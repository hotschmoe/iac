// ignore_for_file: avoid_print
// End-to-end check of the single-player bundle in a real browser engine:
// serves the built bundle from a plain static file server, drives it in
// headless Chromium over the DevTools protocol with real mouse and key
// input, and watches the wire frames between the page and the engine
// worker. No Flutter, no packages beyond the Dart SDK.
//
//   scripts/play.sh --single-player --no-serve
//   cd clients/web && dart run tool/solo_e2e.dart
//       [--bundle build/single-player] [--port 7881]
//       [--chromium /usr/lib/chromium/chromium] [--shots <dir>] [--software] [--trace]
//       [--server ../../target/debug/iac-server]
//
// --trace prints when the first load asked for each file.
// --software starts Chromium without WebGL, which makes Flutter fall back
// from its WebAssembly build to its JavaScript one: the other way a browser
// may run the same bundle.
//
// It plays one blitz world: new game, the homeworld on screen, an order
// accepted and completed, the bots on the board and acting, a reload that
// restores the world, a two-hour absence caught up and reported with the
// page staying responsive, a stall of the running page, a second tab kept
// out, and abandoning the save. The worker's clock is moved from here (its
// `Date.now` is shifted before it starts), so an absence takes seconds.
// Last, when an `iac-server` binary is there (--server), it serves the same
// bundle from it on the next port and checks that the page plays
// multiplayer over a WebSocket. It prints what it measured and exits
// non-zero on the first thing that is not as it should be.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

const absenceSeconds = 2 * 3600;
const stallSeconds = 30 * 60;

class Failure implements Exception {
  Failure(this.message);
  final String message;
}

Never fail(String message) => throw Failure(message);

void check(bool ok, String what) {
  if (!ok) fail(what);
  print('  ok  $what');
}

String arg(List<String> args, String name, String fallback) {
  final at = args.indexOf(name);
  return at >= 0 && at + 1 < args.length ? args[at + 1] : fallback;
}

const _types = {
  'html': 'text/html; charset=utf-8',
  'js': 'text/javascript',
  'mjs': 'text/javascript',
  'json': 'application/json',
  'wasm': 'application/wasm',
  'png': 'image/png',
  'ttf': 'font/ttf',
  'otf': 'font/otf',
};

/// A static file server and nothing else: what any web host does.
class StaticHost {
  StaticHost(this.root);
  final Directory root;
  late final HttpServer _server;

  /// Path and size of every file served.
  final Map<String, int> fetched = {};

  /// When each request arrived, in milliseconds since the epoch.
  final List<(int at, String path, int size)> trace = [];

  Future<void> start(int port) async {
    _server = await HttpServer.bind(InternetAddress.loopbackIPv4, port);
    _server.listen((request) async {
      var path = Uri.decodeComponent(request.uri.path);
      if (path.endsWith('/')) path += 'index.html';
      final file = File('${root.path}$path');
      if (path.contains('..') || !file.existsSync()) {
        request.response.statusCode = HttpStatus.notFound;
        await request.response.close();
        return;
      }
      fetched[path] = file.lengthSync();
      trace.add((DateTime.now().millisecondsSinceEpoch, path, file.lengthSync()));
      request.response.headers
        ..set(HttpHeaders.contentTypeHeader, _types[path.split('.').last] ?? 'application/octet-stream')
        ..set(HttpHeaders.cacheControlHeader, 'no-store');
      await request.response.addStream(file.openRead());
      await request.response.close();
    });
  }

  int get port => _server.port;
  String get url => 'http://127.0.0.1:$port/';
  Future<void> stop() => _server.close(force: true);
}

/// A DevTools protocol connection to one browser, with flat sessions.
class Devtools {
  Devtools(this._socket) {
    _socket.listen((data) {
      final message = jsonDecode(data as String) as Map<String, dynamic>;
      final id = message['id'] as int?;
      if (id == null) {
        _events.add(message);
        return;
      }
      final waiting = _pending.remove(id);
      if (message['error'] != null) {
        waiting?.completeError(StateError('${message['error']}'));
      } else {
        waiting?.complete(message['result'] as Map<String, dynamic>? ?? const {});
      }
    });
  }

  final WebSocket _socket;
  final Map<int, Completer<Map<String, dynamic>>> _pending = {};
  final StreamController<Map<String, dynamic>> _events = StreamController.broadcast();
  int _next = 0;

  Stream<Map<String, dynamic>> get events => _events.stream;

  Future<Map<String, dynamic>> send(String method, {Map<String, dynamic> params = const {}, String? session}) {
    final id = ++_next;
    final done = _pending[id] = Completer();
    _socket.add(jsonEncode({'id': id, 'method': method, 'params': params, 'sessionId': ?session}));
    return done.future.timeout(const Duration(seconds: 30), onTimeout: () => fail('no answer to $method'));
  }
}

/// The page under test and the engine worker it started.
class Browser {
  Browser(this._devtools, this._target, this._page, this._shots);
  final Devtools _devtools;
  final String _target;
  final String _page;
  final Directory _shots;
  String? _worker;

  /// Milliseconds the worker's clock runs ahead of the real one.
  int skewMs = 0;

  /// Every URL the page asked the network for.
  final Set<String> requested = {};

  /// The WebSockets the page opened, and the text frames they received.
  final List<String> sockets = [];
  final List<String> socketFrames = [];

  static const _instrument = r'''
(() => {
  const tap = (window.__solo = {
    statuses: [], last: {}, counts: {}, events: [], errors: [], boards: [], sent: [],
    firstFrame: null, maxGap: 0, lastFrame: 0,
  });
  window.addEventListener('flutter-first-frame', () => { tap.firstFrame = Date.now(); });
  const frame = (t) => {
    if (tap.lastFrame) tap.maxGap = Math.max(tap.maxGap, t - tap.lastFrame);
    tap.lastFrame = t;
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  const Real = window.Worker;
  window.Worker = function (url, options) {
    const worker = new Real(url, options);
    if (!String(url).includes('solo/worker.js')) return worker;
    worker.addEventListener('message', (e) => {
      const m = e.data;
      if (m.type === 'status') tap.statuses.push({ at: Date.now(), ...JSON.parse(m.text) });
      if (m.type !== 'frame') return;
      const f = JSON.parse(m.text);
      tap.counts[f.type] = (tap.counts[f.type] || 0) + 1;
      tap.last[f.type] = f;
      if (f.type === 'error') tap.errors.push(f);
      if (f.type === 'leaderboard') tap.boards.push(f);
      if (f.type === 'event') tap.events.push(f);
      if (f.type === 'tick_update') for (const event of f.events || []) tap.events.push(event);
    });
    const post = worker.postMessage.bind(worker);
    worker.postMessage = (m, ...rest) => {
      tap.sent.push(m.type === 'frame' ? JSON.parse(m.text) : { control: m.type });
      return post(m, ...rest);
    };
    return worker;
  };
})();
''';

  static const _screen = r'''
[...document.querySelectorAll('flt-semantics')].map((e) => {
  const r = e.getBoundingClientRect();
  const own = [...e.childNodes].filter((n) => n.nodeType === 3 || n.nodeName === 'SPAN').map((n) => n.textContent).join(' ');
  return {
    text: ((e.getAttribute('aria-label') || '') + ' ' + own).trim(),
    button: e.getAttribute('role') === 'button' || e.hasAttribute('flt-tappable'),
    x: r.x + r.width / 2, y: r.y + r.height / 2, area: r.width * r.height,
  };
}).filter((n) => n.text && n.area > 0)
''';

  static Future<Browser> open(Devtools devtools, Directory shots) async {
    final target = await devtools.send('Target.createTarget', params: {'url': 'about:blank'});
    final attached = await devtools
        .send('Target.attachToTarget', params: {'targetId': target['targetId'], 'flatten': true});
    final browser = Browser(devtools, target['targetId'] as String, attached['sessionId'] as String, shots);
    devtools.events.listen(browser._onEvent);
    await browser._send('Page.enable');
    await browser._send('Runtime.enable');
    await browser._send('Network.enable');
    await browser._send('Emulation.setFocusEmulationEnabled', {'enabled': true});
    await browser._send('Emulation.setDeviceMetricsOverride',
        {'width': 1440, 'height': 900, 'deviceScaleFactor': 1, 'mobile': false});
    await browser._send('Page.addScriptToEvaluateOnNewDocument', {'source': _instrument});
    await browser._send('Target.setAutoAttach', {'autoAttach': true, 'waitForDebuggerOnStart': true, 'flatten': true});
    return browser;
  }

  Future<Map<String, dynamic>> _send(String method, [Map<String, dynamic> params = const {}]) =>
      _devtools.send(method, params: params, session: _page);

  /// A worker starts paused: shift its clock, then let it run.
  Future<void> _onEvent(Map<String, dynamic> event) async {
    if (event['sessionId'] != _page) return;
    switch (event['method']) {
      case 'Runtime.exceptionThrown':
        print('  page exception: ${jsonEncode(event['params']['exceptionDetails'])}');
      case 'Network.requestWillBeSent':
        requested.add('${event['params']['request']['url']}');
      case 'Network.webSocketCreated':
        sockets.add('${event['params']['url']}');
      case 'Network.webSocketFrameReceived':
        socketFrames.add('${event['params']['response']['payloadData']}');
    }
    if (event['method'] != 'Target.attachedToTarget') return;
    final session = event['params']['sessionId'] as String;
    final info = event['params']['targetInfo'] as Map<String, dynamic>;
    try {
      if (info['type'] == 'worker' && '${info['url']}'.contains('solo/worker.js')) {
        _worker = session;
        await _devtools.send('Runtime.evaluate', session: session, params: {
          'expression':
              '(() => { const real = Date.now.bind(Date); self.__skew = $skewMs; Date.now = () => real() + self.__skew; })()',
        });
      }
      await _devtools.send('Runtime.runIfWaitingForDebugger', session: session);
    } on StateError {
      // The target went away before it could be resumed (a reload).
    }
  }

  /// Move the running worker's clock forward, as a sleeping machine does.
  Future<void> jumpClock(int ms) async {
    skewMs += ms;
    await _devtools.send('Runtime.evaluate', session: _worker, params: {'expression': 'self.__skew = $skewMs'});
  }

  Future<dynamic> eval(String expression) async {
    final result = await _send('Runtime.evaluate', {'expression': expression, 'returnByValue': true, 'awaitPromise': true});
    if (result['exceptionDetails'] != null) fail('page script failed: ${jsonEncode(result['exceptionDetails'])}\n$expression');
    return (result['result'] as Map<String, dynamic>)['value'];
  }

  Future<dynamic> tap(String expression) => eval('(() => { const t = window.__solo; return $expression; })()');

  Future<void> goto(String url) async {
    _worker = null;
    await _send('Page.navigate', {'url': url});
  }

  Future<void> reload() async {
    _worker = null;
    await _send('Page.reload');
  }

  Future<void> close() => _devtools.send('Target.closeTarget', params: {'targetId': _target});

  /// Flutter draws to a canvas; its semantics tree says what is on it.
  Future<void> _wakeSemantics() => eval(
      "(() => { if (!document.querySelector('flt-semantics')) document.querySelector('flt-semantics-placeholder')?.click(); })()");

  Future<List<Map<String, dynamic>>> screen() async {
    await _wakeSemantics();
    return ((await eval(_screen)) as List).cast<Map<String, dynamic>>();
  }

  Future<String> text() async => (await screen()).map((n) => n['text']).join('\n');

  Future<void> shot(String name) async {
    final result = await _send('Page.captureScreenshot', {'format': 'png'});
    File('${_shots.path}/$name.png').writeAsBytesSync(base64Decode(result['data'] as String));
  }

  Future<T> until<T>(String what, Future<T?> Function() probe, {Duration timeout = const Duration(seconds: 60)}) async {
    final deadline = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(deadline)) {
      final found = await probe();
      if (found != null && found != false) return found;
      await Future<void>.delayed(const Duration(milliseconds: 150));
    }
    await shot('failed');
    final status = await eval('JSON.stringify(window.__solo?.statuses.at(-1) ?? null)');
    fail('timed out waiting for $what\n  last status: $status\n  on screen: ${(await text()).replaceAll('\n', ' | ')}');
  }

  Future<void> untilText(String wanted, {Duration timeout = const Duration(seconds: 60)}) => until(
      '"$wanted" on screen', () async => (await text()).toLowerCase().contains(wanted.toLowerCase()),
      timeout: timeout);

  /// A real click on the control whose label contains [label].
  Future<void> click(String label) async {
    final node = await until('a control labelled "$label"', () async {
      final matches = [
        for (final n in await screen())
          if ((n['text'] as String).toLowerCase().contains(label.toLowerCase())) n,
      ]..sort((a, b) {
          final byRole = (b['button'] == true ? 1 : 0) - (a['button'] == true ? 1 : 0);
          return byRole != 0 ? byRole : (a['area'] as num).compareTo(b['area'] as num);
        });
      return matches.isEmpty ? null : matches.first;
    });
    final at = {'x': node['x'], 'y': node['y'], 'button': 'left', 'clickCount': 1};
    await _send('Input.dispatchMouseEvent', {'type': 'mouseMoved', ...at, 'button': 'none'});
    await _send('Input.dispatchMouseEvent', {'type': 'mousePressed', ...at});
    await _send('Input.dispatchMouseEvent', {'type': 'mouseReleased', ...at});
  }

  Future<void> wheel(int deltaY) =>
      _send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': 720, 'y': 500, 'deltaX': 0, 'deltaY': deltaY});

  Future<void> press(String key, String code, int keyCode, {String? text}) async {
    final base = {'key': key, 'code': code, 'windowsVirtualKeyCode': keyCode};
    await _send('Input.dispatchKeyEvent', {'type': text == null ? 'rawKeyDown' : 'keyDown', ...base, 'text': ?text});
    await _send('Input.dispatchKeyEvent', {'type': 'keyUp', ...base});
  }

  Future<void> type(String text) async {
    for (final char in text.split('')) {
      final upper = char.toUpperCase();
      final code = char == ' ' ? 'Space' : 'Key$upper';
      await press(char, code, upper.codeUnitAt(0), text: char);
    }
  }

  Future<Map<String, dynamic>> status() async =>
      (jsonDecode(await eval('JSON.stringify(window.__solo.statuses.at(-1) ?? {})') as String) as Map<String, dynamic>);

  Future<Map<String, dynamic>> untilStatus(String what, bool Function(Map<String, dynamic> status) wanted,
          {Duration timeout = const Duration(seconds: 60)}) =>
      until('engine status: $what', () async {
        final s = await status();
        return wanted(s) ? s : null;
      }, timeout: timeout);

  Future<Map<String, dynamic>?> last(String type) async {
    final text = await eval('JSON.stringify(window.__solo.last[${jsonEncode(type)}] ?? null)') as String;
    return jsonDecode(text) as Map<String, dynamic>?;
  }
}

int level(Map<String, dynamic> homeworld, String building) =>
    ((homeworld['buildings'] as List).firstWhere((b) => b['building_type'] == building) as Map<String, dynamic>)['level']
        as int;

String megabytes(num bytes) => '${(bytes / 1048576).toStringAsFixed(2)} MB';

/// What a live tick cost in the worker since this page loaded: the mean of
/// the worker's ten-second means, and the worst single tick.
Future<String> liveTickCost(Browser browser) async {
  final samples = (jsonDecode(await browser.eval(
          'JSON.stringify(window.__solo.statuses.filter((s) => s.tick_ms > 0).map((s) => [s.tick_ms, s.tick_ms_max]))') as String)
      as List);
  if (samples.isEmpty) return 'not sampled';
  final mean = samples.map((s) => (s as List)[0] as num).reduce((a, b) => a + b) / samples.length;
  final worst = samples.map((s) => (s as List)[1] as num).reduce((a, b) => a > b ? a : b);
  return '${mean.toStringAsFixed(2)} ms mean, ${worst.toStringAsFixed(2)} ms worst tick (${samples.length} ten-second samples)';
}

Future<void> main(List<String> args) async {
  final bundle = Directory(arg(args, '--bundle', 'build/single-player'));
  if (!File('${bundle.path}/solo/iac_solo.wasm').existsSync()) {
    stderr.writeln('no single-player bundle in ${bundle.path}: run scripts/play.sh --single-player --no-serve');
    exit(2);
  }
  final software = args.contains('--software');
  final shots = Directory(arg(args, '--shots', '${Directory.systemTemp.path}/iac-solo-e2e-shots'))
    ..createSync(recursive: true);

  final host = StaticHost(bundle);
  await host.start(int.parse(arg(args, '--port', '7881')));
  final profile = Directory.systemTemp.createTempSync('iac-solo-e2e-profile');
  final process = await Process.start(arg(args, '--chromium', '/usr/lib/chromium/chromium'), [
    '--headless=new',
    '--remote-debugging-port=0',
    '--user-data-dir=${profile.path}',
    '--no-first-run',
    '--no-default-browser-check',
    '--window-size=1440,900',
    // No host but this machine exists for the page: the bundle has to be
    // enough on its own.
    '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1',
    '--no-proxy-server',
    // With WebGL the page runs as a browser with a GPU runs it (the
    // dart2wasm build on skwasm); without, Flutter falls back to its
    // JavaScript build on CanvasKit.
    if (!software) ...['--use-gl=angle', '--use-angle=gl-egl', '--ignore-gpu-blocklist'],
    'about:blank',
  ]);
  final endpoint = Completer<String>();
  process.stderr.transform(utf8.decoder).transform(const LineSplitter()).listen((line) {
    final at = line.indexOf('ws://');
    if (line.contains('DevTools listening on') && at >= 0 && !endpoint.isCompleted) endpoint.complete(line.substring(at));
  });
  unawaited(process.stdout.drain<void>());

  var code = 0;
  try {
    final devtools = Devtools(await WebSocket.connect(
        await endpoint.future.timeout(const Duration(seconds: 20), onTimeout: () => fail('Chromium did not start'))));
    await play(devtools, host, bundle, shots,
        software: software, trace: args.contains('--trace'), server: arg(args, '--server', '../../target/debug/iac-server'));
  } on Failure catch (failure) {
    stderr.writeln('SOLO E2E FAIL: ${failure.message}');
    code = 1;
  } finally {
    process.kill();
    await process.exitCode;
    await host.stop();
    profile.deleteSync(recursive: true);
  }
  exit(code);
}

/// The bundle is one client for both worlds: served by a real `iac-server`
/// and given a player name in the URL, it logs in over a WebSocket and
/// leaves the engine worker alone.
Future<void> multiplayer(Browser browser, Directory bundle, String server, int port) async {
  final data = Directory.systemTemp.createTempSync('iac-solo-e2e-server');
  final process = await Process.start(server,
      ['--host', '127.0.0.1', '--port', '$port', '--db', '${data.path}/world.db', '--web-dir', bundle.path, '--pace', 'blitz']);
  unawaited(process.stdout.drain<void>());
  unawaited(process.stderr.drain<void>());
  try {
    await browser.until('the server to listen on port $port', () async {
      try {
        (await Socket.connect('127.0.0.1', port)).destroy();
        return true;
      } on SocketException {
        return false;
      }
    });
    await browser.goto('http://127.0.0.1:$port/?name=Wire');
    await browser.until('a full_state over a WebSocket',
        () async => browser.socketFrames.any((frame) => frame.contains('"type":"full_state"')));
    check(browser.sockets.length == 1 && browser.sockets.single == 'ws://127.0.0.1:$port/ws', 'the page opened one WebSocket, to the server');
    check((await browser.eval('window.__solo.statuses.length')) == 0, 'the engine worker was not started');
    await browser.until('tick updates from the server',
        () async => browser.socketFrames.where((frame) => frame.contains('"type":"tick_update"')).length >= 3);
    await browser.untilText('wire');
    check(true, 'the console is live on the server as Wire');
    await browser.shot('11-multiplayer');
  } finally {
    process.kill();
    await process.exitCode;
    data.deleteSync(recursive: true);
  }
}

/// The whole check, on a browser with nothing in it.
Future<void> play(Devtools devtools, StaticHost host, Directory bundle, Directory shots,
    {required bool software, required bool trace, required String server}) async {
  final started = Stopwatch()..start();
  final browser = await Browser.open(devtools, shots);
  final engine = File('${bundle.path}/solo/iac_solo.wasm');
  final files = bundle.listSync(recursive: true).whereType<File>().toList();
  final total = files.fold<int>(0, (sum, f) => sum + f.lengthSync());
  print('bundle ${bundle.path}: ${files.length} files, ${megabytes(total)}; engine iac_solo.wasm '
      '${megabytes(engine.lengthSync())} (${megabytes(gzip.encode(engine.readAsBytesSync()).length)} gzipped)');

  // A fresh headless Chromium can sit for many seconds before its very
  // first request. That wait is the browser's, so it is spent on a file
  // that is not the page, and the page is timed after it.
  final warmUp = Stopwatch()..start();
  await browser.goto('${host.url}version.json');
  await browser.until('the browser to make its first request', () async => host.fetched.containsKey('/version.json'));
  print('browser made its first request after ${warmUp.elapsedMilliseconds} ms');
  host.fetched.clear();
  host.trace.clear();

  print('first load');
  final navigated = DateTime.now().millisecondsSinceEpoch;
  await browser.goto(host.url);
  final firstFrame = (await browser.until('the first Flutter frame', () => browser.tap('t.firstFrame')) as int) - navigated;
  final idle = await browser.untilStatus('idle', (s) => s['phase'] == 'idle');
  final engineReady = (idle['at'] as int) - navigated;
  await browser.untilText('new game');
  final interactive = DateTime.now().millisecondsSinceEpoch - navigated;
  check(idle['world'] == null && idle['refused'] == null, 'a browser with no save offers a new game');
  check(((idle['options'] as Map)['presets'] as List).any((p) => p['name'] == 'season'), 'the engine lists the pace presets');
  if (trace) {
    for (final (at, path, size) in host.trace) {
      print('${(at - navigated).toString().padLeft(7)} ms  $path  $size');
    }
  }
  final fetchedBytes = host.fetched.values.fold<int>(0, (a, b) => a + b);
  final fetchedFiles = host.fetched.length;
  final client = host.fetched.containsKey('/main.dart.wasm') ? 'dart2wasm on skwasm' : 'dart2js on CanvasKit';
  check(software != host.fetched.containsKey('/main.dart.wasm'), 'Flutter runs its ${software ? 'JavaScript' : 'WebAssembly'} build ($client)');
  final elsewhere = browser.requested.where((u) => u.startsWith('http') && !u.startsWith(host.url)).toList();
  check(elsewhere.every((u) => u.startsWith('https://fonts.gstatic.com/s/roboto/')),
      'the bundle runs with no other host reachable; all it asked elsewhere for is Flutter\'s default Roboto ($elsewhere)');
  await browser.shot('01-new-game');

  print('new game');
  await browser.click('blitz');
  await browser.untilText('always at the controls');
  await browser.click('start');
  final running = await browser.untilStatus('running', (s) => s['phase'] == 'running' && s['world'] != null);
  final world = running['world'] as Map<String, dynamic>;
  check(world['commander'] == 'Admiral' && world['preset'] == 'blitz' && world['bots'] == 8,
      'the world is Admiral\'s, at blitz pace, with 8 bots');
  final first = await browser.until('the first full_state', () => browser.last('full_state'));
  check(first['player']['name'] == 'Admiral' && (first['fleets'] as List).isNotEmpty, 'full_state carries the commander and a fleet');
  check(level(first['homeworld'] as Map<String, dynamic>, 'MetalMine') == 1, 'the homeworld starts with a level 1 Metal Mine');
  final home = first['player']['homeworld'];
  await browser.untilText('admiral');
  await browser.untilText('metal');
  await browser.untilText('queue a building');
  check(true, 'the homeworld is on screen, and the guide starts for the new commander');
  await browser.shot('02-homeworld');
  check((await browser.tap('t.sent.some((m) => m.type === "auth" && !("token" in m))')) == true, 'the client logged in with the wire protocol, no token');

  print('an order');
  await browser.press('/', 'Slash', 191, text: '/');
  await browser.type('build metal');
  await browser.press('Enter', 'Enter', 13, text: '\r');
  await browser.until('the build command on the wire',
      () => browser.tap('t.sent.some((m) => m.type === "command" && m.action === "build" && m.building_type === "MetalMine")'));
  await browser.until('the Metal Mine to finish', () => browser.tap(
      't.events.some((e) => e.kind === "BuildingCompleted" && e.building_type === "MetalMine" && e.new_level === 2)'));
  final after = (await browser.last('tick_update'))!;
  check(level(after['homeworld_update'] as Map<String, dynamic>, 'MetalMine') == 2, 'the order was accepted and completed: Metal Mine level 2');
  check((await browser.tap('t.errors.length')) == 0, 'no error frame so far');
  await browser.shot('03-order-done');

  print('bots');
  final board = (jsonDecode(await browser.until('a leaderboard', () => browser.tap('t.boards.length ? JSON.stringify(t.boards[0]) : null')) as String)
      as Map<String, dynamic>);
  final bots = (board['bots'] as List).cast<Map<String, dynamic>>();
  check(bots.length == 8 && bots.every((b) => (b['name'] as String).startsWith('BOT-')), 'the board lists 8 bots apart from the ranking');
  check((board['entries'] as List).length == 1, 'the commander is the only ranked player');
  await browser.untilText('BOT-');
  await browser.until('a bot to raise its score', () => browser.tap('''(() => {
    const before = new Map(t.boards[0].bots.map((b) => [b.name, b.score]));
    return t.boards.at(-1).bots.some((b) => b.score > before.get(b.name) + 0.01);
  })()'''), timeout: const Duration(seconds: 150));
  check(true, 'bots act: a later board shows a bot with a higher score');
  await browser.wheel(2400);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shot('04-bots');
  await browser.wheel(-2400);
  final early = await liveTickCost(browser);

  print('a second tab');
  final second = await Browser.open(devtools, shots);
  await second.goto(host.url);
  await second.untilStatus('locked', (s) => s['phase'] == 'locked');
  await second.untilText('open in another tab');
  check(true, 'a second tab is told the world is open elsewhere and does not run it');
  await second.shot('04b-second-tab');
  await second.close();

  print('reload');
  await Future<void>.delayed(const Duration(seconds: 3));
  final before = (await browser.last('tick_update'))!;
  await browser.reload();
  final held = await browser.untilStatus('held', (s) => s['phase'] == 'held');
  final saved = held['world'] as Map<String, dynamic>;
  check(saved['seed'] == world['seed'] && saved['commander'] == 'Admiral' && saved['bots'] == 8, 'the reloaded page finds the same world saved');
  check((saved['tick'] as int) > 0 && (saved['tick'] as int) <= (before['tick'] as int) + 5,
      'the save is from before the reload (tick ${saved['tick']}; the page was last seen at ${before['tick']})');
  await browser.untilText('saved world');
  await browser.shot('05-saved');
  await browser.click('continue');
  final restored = await browser.until('the restored full_state', () => browser.last('full_state'));
  check(restored['tick'] as int >= (before['tick'] as int), 'the world carried on to tick ${restored['tick']} (was ${before['tick']})');
  check(level(restored['homeworld'] as Map<String, dynamic>, 'MetalMine') == 2 && '${restored['player']['homeworld']}' == '$home',
      'the same homeworld, with the Metal Mine still at level 2');
  await browser.untilText('admiral');
  await browser.shot('06-restored');

  print('absence of ${absenceSeconds ~/ 3600} hours');
  await Future<void>.delayed(const Duration(seconds: 3));
  final leftAt = (await browser.last('tick_update'))!['tick'] as int;
  browser.skewMs += absenceSeconds * 1000;
  await browser.reload();
  final away = await browser.untilStatus('held', (s) => s['phase'] == 'held');
  final behind = away['world']['behind'] as int;
  check(behind >= absenceSeconds && behind < absenceSeconds + 300, 'the save is $behind ticks behind the clock');
  await browser.untilText('away 2h');
  await browser.click('continue');
  await browser.untilStatus('catching up', (s) => s['world']?['catch_up'] != null);
  await browser.eval('window.__solo.maxGap = 0');
  await browser.untilText('catching up');
  await browser.shot('07-catching-up');
  final arrived = await browser.untilStatus('caught up', (s) => s['phase'] == 'running' && s['world']['catch_up'] == null,
      timeout: const Duration(minutes: 5));
  final gap = (await browser.tap('t.maxGap') as num).round();
  final progress = (jsonDecode(await browser.eval(
          'JSON.stringify(window.__solo.statuses.filter((s) => s.world?.catch_up).map((s) => [s.at, s.world.catch_up.played]))') as String)
      as List);
  final seconds = ((progress.last[0] as num) - (progress.first[0] as num)) / 1000;
  final rate = seconds > 0 ? ((progress.last[1] as num) - (progress.first[1] as num)) / seconds : double.nan;
  final notices = (arrived['world']['notices'] as List).cast<String>();
  check(notices.isNotEmpty && notices.first.startsWith('Away 2h'), 'the engine reports the absence: ${notices.isEmpty ? '' : notices.first}');
  check(gap < 250, 'the page kept drawing while the engine caught up (longest gap between frames $gap ms)');
  await browser.untilText('while you were away');
  await browser.shot('08-away-report');
  await browser.click('take command');
  final back = await browser.until('the full_state after the absence', () => browser.last('full_state'));
  final played = (back['tick'] as int) - leftAt;
  check(played >= absenceSeconds && played < absenceSeconds + 300, 'the world is $played ticks further on');
  await browser.until('the report as alerts on the wire',
      () => browser.tap('t.events.some((e) => e.kind === "Alert" && e.message.startsWith("Away 2h"))'));
  check(true, 'the report reached the client as Alert events');
  await browser.untilText('admiral');
  await browser.shot('09-after-absence');
  await Future<void>.delayed(const Duration(seconds: 25));
  final late = await liveTickCost(browser);

  print('a stall of ${stallSeconds ~/ 60} minutes with the page open');
  final states = await browser.tap('t.counts.full_state') as int;
  await browser.jumpClock(stallSeconds * 1000);
  await browser.until('a fresh full_state after the stall', () async => (await browser.tap('t.counts.full_state') as int) > states,
      timeout: const Duration(minutes: 3));
  check((await browser.tap('t.events.some((e) => e.kind === "Alert" && e.message.startsWith("Away 30m"))')) == true,
      'the open session was brought up to date and told');
  check((await browser.tap('t.errors.length')) == 0, 'no error frame in the whole run');

  print('abandon');
  await browser.reload();
  await browser.untilStatus('held', (s) => s['phase'] == 'held');
  await browser.click('abandon this world');
  await browser.untilText('cannot be brought back');
  await browser.shot('10-abandon-confirm');
  await browser.click('delete the save');
  await browser.untilStatus('idle again', (s) => s['phase'] == 'idle' && s['world'] == null);
  final left = await browser.eval('''new Promise((resolve) => {
    const open = indexedDB.open('iac-solo');
    open.onsuccess = () => {
      const all = open.result.transaction('saves').objectStore('saves').getAllKeys();
      all.onsuccess = () => resolve(all.result.length);
    };
  })''');
  check(left == 0, 'the save is gone from IndexedDB');
  await browser.untilText('new game');

  if (File(server).existsSync()) {
    print('the same bundle against a real server');
    await multiplayer(browser, bundle, server, host.port + 1);
  } else {
    print('no server binary at $server: the multiplayer path of this bundle was NOT checked (cargo build -p iac-server)');
  }

  print('');
  print('measured (headless Chromium, $client, blitz world, 8 hands-on bots)');
  print('  first load fetched       $fetchedFiles files, ${megabytes(fetchedBytes)} uncompressed');
  print('  first Flutter frame      $firstFrame ms after navigation');
  print('  engine ready (wasm, db)  $engineReady ms');
  print('  new-game form usable     $interactive ms (the boot animation runs first)');
  print('  live tick, first minute  $early');
  print('  live tick, two hours in  $late');
  print('  catch-up                 ${progress.last[1]} ticks in ${seconds.toStringAsFixed(1)} s: ${rate.round()} ticks/s; '
      'longest frame gap $gap ms');
  print('  screenshots              ${shots.path}');
  print('SOLO E2E OK in ${started.elapsed.inSeconds} s');
}
