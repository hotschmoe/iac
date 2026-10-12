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
// It plays one blitz world: new game with no other host asked for anything,
// the homeworld on screen, an order accepted and completed, the offer of
// browser notices answered from its own button, toasts that stay whole
// while their neighbours leave, a hold loaded from the stockpile at the dock
// and unloaded again, a jump called off from the Stop tile while the
// windshield still plays it, standing orders given through their panel with the
// fleet holding for the reason the engine gives and then set back to manual,
// the founding panel of an outpost opened where the starter fleet stands,
// with the engine's survey and its reason why none can be founded there
// (founding itself takes a convoy in rings 9 to 20 and is not played),
// a ship batch the stockpile does not cover refused with
// nothing charged and its order button off, the engine's refusal of a typed
// order shown on screen, a building queued ahead of its resources shown
// with its shortfall at desktop and phone width, every screen pictured at
// both widths, the bots on the board and acting, a reload that restores the
// world, a two-hour absence caught up and reported with the page staying
// responsive, a stall of the running page, a second tab kept out,
// abandoning the save, and the same page from an origin that is not a
// secure context, where no tab can be kept out and the save cannot be
// marked to keep, and the front door says both.
// The worker's clock is moved from here (its `Date.now` is shifted before
// it starts), so an absence takes seconds.
// Last, when an `iac-server` binary is there (--server), it serves the same
// bundle from it on the next port and checks that the page plays
// multiplayer over a WebSocket. It prints what it measured and exits
// non-zero on the first thing that is not as it should be.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

const absenceSeconds = 2 * 3600;
const stallSeconds = 30 * 60;

/// A name for this machine that is not `localhost`: a page from it over
/// plain http is no secure context, as on a LAN.
const plainHost = 'iac-lan.test';

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

  /// What the page asked a host other than [host] for.
  List<String> elsewhere(StaticHost host) => [
        for (final url in requested)
          if (url.startsWith('http') && !url.startsWith(host.url)) url,
      ];

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

  static const _toasts = r'''
[...document.querySelectorAll('flt-semantics')].map((e) => {
  const own = [...e.childNodes].filter((n) => n.nodeType === 3 || n.nodeName === 'SPAN').map((n) => n.textContent).join(' ');
  return { text: ((e.getAttribute('aria-label') || '') + ' ' + own).trim(), width: e.getBoundingClientRect().width };
}).filter((n) => /^(Command|Complete|Refused)\. /.test(n.text))
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

  /// The command, completion and refusal toasts on screen: text and width.
  Future<List<Map<String, dynamic>>> toasts() async => ((await eval(_toasts)) as List).cast<Map<String, dynamic>>();

  Future<void> shot(String name) async {
    final result = await _send('Page.captureScreenshot', {'format': 'png'});
    File('${_shots.path}/$name.png').writeAsBytesSync(base64Decode(result['data'] as String));
  }

  /// One picture at desktop width and one at a phone's.
  Future<void> shotBoth(String name) async {
    await shot('$name-1440');
    await resize(390, 844);
    await shot('$name-390');
    await resize(1440, 900);
  }

  /// The viewport of a desktop window or of a phone held upright.
  Future<void> resize(int width, int height) async {
    await _send('Emulation.setDeviceMetricsOverride', {'width': width, 'height': height, 'deviceScaleFactor': 1, 'mobile': false});
    await Future<void>.delayed(const Duration(milliseconds: 700));
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

  /// The control whose label contains [label], once it is on screen.
  Future<Map<String, dynamic>> find(String label) => until('a control labelled "$label"', () async {
        final matches = [
          for (final n in await screen())
            if ((n['text'] as String).toLowerCase().contains(label.toLowerCase())) n,
        ]..sort((a, b) {
            final byRole = (b['button'] == true ? 1 : 0) - (a['button'] == true ? 1 : 0);
            return byRole != 0 ? byRole : (a['area'] as num).compareTo(b['area'] as num);
          });
        return matches.isEmpty ? null : matches.first;
      });

  /// A real click where [node] was found.
  Future<void> clickAt(Map<String, dynamic> node) async {
    final at = {'x': node['x'], 'y': node['y'], 'button': 'left', 'clickCount': 1};
    await _send('Input.dispatchMouseEvent', {'type': 'mouseMoved', ...at, 'button': 'none'});
    await _send('Input.dispatchMouseEvent', {'type': 'mousePressed', ...at});
    await _send('Input.dispatchMouseEvent', {'type': 'mouseReleased', ...at});
  }

  /// A real click on the control whose label contains [label].
  Future<void> click(String label) async => clickAt(await find(label));

  Future<void> wheel(int deltaY, {int x = 720}) =>
      _send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': 500, 'deltaX': 0, 'deltaY': deltaY});

  Future<void> press(String key, String code, int keyCode, {String? text}) async {
    final base = {'key': key, 'code': code, 'windowsVirtualKeyCode': keyCode};
    await _send('Input.dispatchKeyEvent', {'type': text == null ? 'rawKeyDown' : 'keyDown', ...base, 'text': ?text});
    await _send('Input.dispatchKeyEvent', {'type': 'keyUp', ...base});
  }

  Future<void> type(String text) async {
    for (final char in text.split('')) {
      final upper = char.toUpperCase();
      final code = char == ' ' ? 'Space' : RegExp(r'\d').hasMatch(char) ? 'Digit$char' : 'Key$upper';
      await press(char, code, upper.codeUnitAt(0), text: char);
    }
  }

  static const _typing = '''(() => {
      let at = document.activeElement;
      while (at && at.shadowRoot && at.shadowRoot.activeElement) at = at.shadowRoot.activeElement;
      return !!at && (at.tagName === 'INPUT' || at.tagName === 'TEXTAREA');
    })()''';

  /// Open the `/` palette, wait until its field takes the keys, type
  /// [text], submit it and wait until the field has let the keys go again:
  /// a second command typed while the first field is still closing would
  /// lose its Enter.
  Future<void> command(String text) async {
    await press('/', 'Slash', 191, text: '/');
    await until('the command palette to take the keyboard', () => eval(_typing));
    await type(text);
    await press('Enter', 'Enter', 13, text: '\r');
    await until('the command palette to let the keyboard go', () async => (await eval(_typing)) == false);
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
    // enough on its own. One more name leads here, for the plain-http check.
    '--host-resolver-rules=MAP $plainHost 127.0.0.1, MAP * ~NOTFOUND, EXCLUDE 127.0.0.1',
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
      ['--host', '127.0.0.1', '--port', '$port', '--world', '${data.path}/world.iac', '--web-dir', bundle.path, '--pace', 'blitz']);
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
    final elsewhere = [
      for (final url in browser.requested)
        if (url.startsWith('http') && !url.startsWith('http://127.0.0.1:')) url,
    ];
    check(elsewhere.isEmpty, 'and as a multiplayer client it asked no other host for anything either ($elsewhere)');
    await browser.shot('11-multiplayer');
  } finally {
    process.kill();
    await process.exitCode;
    data.deleteSync(recursive: true);
  }
}

/// A toast slides in once. Three raised one after another leave one after
/// another, and while one leaves the others must stay as wide as they were.
Future<void> toastsStayWhole(Browser browser) async {
  print('toasts');
  await browser.until('the earlier toasts to leave', () async => (await browser.toasts()).isEmpty);
  for (final command in ['status', 'fleet', 'help']) {
    await browser.command(command);
  }
  final widest = <String, num>{};
  var together = 0;
  String? shrank;
  await browser.until('the three toasts to come and go', () async {
    final now = await browser.toasts();
    if (now.length > together) together = now.length;
    for (final toast in now) {
      final text = toast['text'] as String, width = toast['width'] as num, was = widest[text] ?? 0;
      if (width + 1 < was) shrank ??= '"$text" went from ${was.round()} to ${width.round()} px';
      if (width > was) widest[text] = width;
    }
    return now.isEmpty && widest.isNotEmpty;
  }, timeout: const Duration(seconds: 20));
  check(together == 3, 'three toasts were up together');
  check(shrank == null, 'each stayed whole while the others left${shrank == null ? '' : ': $shrank'}');
}

/// The player's first fleet in the newest tick update.
Future<Map<String, dynamic>> firstFleet(Browser browser) async =>
    ((await browser.last('tick_update'))!['fleets'] as List).first as Map<String, dynamic>;

/// A new empire's two scouts stand at the home dock with an empty hold and
/// the stockpile holds 100 deuterium, which no mine adds to yet. The Storage
/// tab's hold panel loads it, the dock leaves it aboard, the panel says so
/// at desktop width and at a phone's, and a typed `unload` puts it back.
Future<void> loadHold(Browser browser) async {
  print('loading a hold');
  await browser.click('homeworld');
  await browser.untilText('build slot a');
  await browser.press('5', 'Digit5', 53, text: '5');
  await browser.untilText('load a hold');
  final empty = await firstFleet(browser);
  final hold = (empty['cargo_capacity'] as num).floor();
  check(empty['laden'] == false && (empty['cargo']['deuterium'] as num) == 0 && hold > 100, 'the fleet is docked with an empty hold of $hold');
  check((await browser.text()).contains('0 / $hold'), 'the panel shows what is aboard and the size of the hold');
  await browser.shot('03a-hold-empty-1440');

  await browser.click('deuterium 100 in the stockpile');
  await browser.click('load 100 de');
  await browser.until('the load on the wire',
      () => browser.tap('t.sent.some((m) => m.action === "load" && m.resource === "deuterium" && !("amount" in m))'));
  await browser.until('the hold loaded and marked', () async {
    final fleet = await firstFleet(browser);
    return fleet['laden'] == true && (fleet['cargo']['deuterium'] as num) == 100;
  });
  check((await browser.tap('t.events.some((e) => e.kind === "Alert" && e.message.includes("loaded 100 deuterium from the stockpile"))')) == true,
      'the engine moved 100 deuterium into the hold and said so');
  check(((await stockpile(browser))['deuterium'] as num) == 0, 'out of the stockpile');
  await Future<void>.delayed(const Duration(seconds: 4));
  final later = await firstFleet(browser);
  check(later['laden'] == true && (later['cargo']['deuterium'] as num) == 100, 'four ticks later the dock has left it aboard');
  await browser.untilText('laden');
  check((await browser.text()).contains('100 / $hold'), 'the panel shows the laden hold');
  await browser.shot('03a-hold-laden-1440');
  await browser.resize(390, 844);
  await browser.wheel(600, x: 195);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shot('03a-hold-laden-390');
  await browser.wheel(-600, x: 195);
  await browser.resize(1440, 900);

  await browser.command('unload');
  await browser.until('the unload on the wire', () => browser.tap('t.sent.some((m) => m.action === "unload")'));
  await browser.until('the hold empty again', () async {
    final fleet = await firstFleet(browser);
    return fleet['laden'] == false && (fleet['cargo']['deuterium'] as num) == 0;
  });
  check(((await stockpile(browser))['deuterium'] as num) == 100, 'a typed unload put the 100 deuterium back');
  check((await browser.tap('t.errors.length')) == 0, 'no error frame so far');
  await browser.click('overview');
  await browser.untilText('admiral');
}

/// Standing orders through the panel, against the engine itself. P opens it
/// on a fleet flown by hand. A patrol of one hex that fights only at ten to
/// one is chosen with the steppers and set: the wire carries what the typed
/// `policy patrol range=1 engage=10.0` would, the engine confirms the orders
/// with the settings they fly with, and the fleet reports what it does under
/// them or why it holds, which is the world's to decide (it has held for its
/// shields and it has attacked a scout in the home sector). The panel shows
/// that sentence as the engine sent it, and for a hold the fleet status does
/// too, at desktop width, in the sheet at 1100 px and on a phone. Manual
/// clears the orders again.
Future<void> standingOrders(Browser browser) async {
  print('standing orders');
  final before = await firstFleet(browser);
  check(before['policy'] == null, 'the fleet is flown by hand');
  await browser.type('p');
  await browser.untilText('no report yet');
  final listed = (await browser.text()).toLowerCase();
  check(['manual', 'prospect', 'mine+return', 'salvage+sites', 'patrol home'].every((d) => listed.contains('doctrine $d')),
      'P opens the standing orders, with the five doctrines and a line for each');
  await browser.shot('21d-orders-manual-1440');

  await browser.click('doctrine patrol home');
  await browser.untilText('least odds to fight');
  for (var i = 0; i < 3; i++) {
    await browser.click('range from home down');
  }
  for (var i = 0; i < 6; i++) {
    await browser.click('least odds to fight up');
  }
  await browser.until('the steppers at one hex and ten to one', () async {
    final text = await browser.text();
    return text.contains('1 HEX') && text.contains('10.0X');
  });
  await browser.click('set patrol home');
  await browser.until('the orders on the wire', () => browser.tap('''t.sent.some((m) => m.type === "policy_update" && m.fleet_id === ${before['id']} &&
      m.preset === "patrol_home" && JSON.stringify(m.params) ===
      JSON.stringify({min_fuel_pct: 10, cargo_return_pct: 85, max_range: 1, engage_ratio_x10: 100, fuel_budget_pct: 20}))'''));
  check(true, 'Set sends the doctrine with its settings, as the typed policy does');

  Future<String> report(String action) async => await browser.until(
      'the engine\'s "$action" report',
      () => browser.tap('t.events.findLast((e) => e.kind === "PolicyAction" && e.action === "$action")?.reason ?? null')) as String;
  final set = await report('engage');
  check(set == 'doctrine set: patrol home (range 1, engage 10.0x, fuel budget 20% of deuterium income)', 'the engine confirms the orders: $set');
  const newest = 'JSON.stringify(t.events.findLast((e) => e.kind === "PolicyAction" && !["engage", "clear"].includes(e.action)) ?? null)';
  final first = jsonDecode(await browser.until('a report of what the fleet does under them',
      () => browser.tap(newest).then((found) => found == 'null' ? null : found)) as String) as Map<String, dynamic>;
  final held = first['action'] == 'hold';
  check('${first['reason']}'.isNotEmpty, held ? 'the fleet holds, and the engine says why: ${first['reason']}' : 'the fleet acts, and the engine says why: ${first['action']}, ${first['reason']}');
  await browser.until('the newest report on screen, in the engine\'s words', () async {
    final now = jsonDecode(await browser.tap(newest) as String) as Map<String, dynamic>;
    final text = await browser.text(), reason = '${now['reason']}';
    if (now['action'] != 'hold') return text.contains('LAST REPORT: ${'${now['action']}'.toUpperCase()}') && text.contains(reason);
    return text.contains('HOLDING FOR') && text.contains('PATROL HOME - HOLDING') && reason.allMatches(text).length >= 2;
  });
  check(true, held ? 'the panel and the fleet status both show that sentence, word for word' : 'the panel shows that sentence, word for word');
  final flown = (await firstFleet(browser))['policy'];
  check(
      jsonEncode(flown) ==
          jsonEncode({
            'preset': 'patrol_home',
            'params': {'min_fuel_pct': 10, 'cargo_return_pct': 85, 'max_range': 1, 'engage_ratio_x10': 100, 'fuel_budget_pct': 20},
          }),
      'the fleet row carries patrol home with the settings in force: ${jsonEncode(flown)}');
  await browser.untilText('range 1 hex - engage 10.0x - fuel 20%');
  check(true, 'and the panel shows them under the doctrine');
  await browser.shot('21d-orders-report-1440');
  await browser.resize(1100, 800);
  await browser.shot('21d-orders-report-1100');
  await browser.resize(390, 844);
  await browser.shot('21d-orders-report-390');
  await browser.resize(1440, 900);

  await browser.click('doctrine manual');
  await browser.click('clear orders');
  await browser.until('manual on the wire',
      () => browser.tap('t.sent.some((m) => m.type === "policy_update" && m.preset === "manual" && !("params" in m))'));
  check(await report('clear') == 'standing orders cleared', 'Manual clears the orders, and the engine says so');
  await browser.until('the fleet flown by hand again', () async => (await firstFleet(browser))['policy'] == null);
  await browser.untilText('last report: clear');
  await browser.press('Escape', 'Escape', 27);
  await browser.until('the orders panel to close', () async => !(await browser.text()).toLowerCase().contains('last report: clear'));
  await browser.until('the fleet to finish a jump its orders had begun', () async => (await firstFleet(browser))['state'] != 'Moving');
  await Future<void>.delayed(const Duration(seconds: 2));
}

/// The windshield's gate keys and the lane each one flies.
const gates = {'w': (0, -1), 'e': (1, -1), 'd': (1, 0), 's': (0, 1), 'a': (-1, 1), 'q': (-1, 0)};

/// A jump called off from the windshield while the windshield still plays
/// it. A scout's jump charges for one tick, and the engine takes a stop for
/// that long and no longer: so the gate key and the Stop tile are pressed
/// one after the other just after a tick, with most of a second before the
/// next. The engine gets the move and then the stop, abandons the jump and
/// says what fuel is forfeit; the fleet stays where it was.
Future<void> callOffAJump(Browser browser) async {
  print('calling off a jump');
  final fleet = await firstFleet(browser);
  final id = fleet['id'] as int;
  final at = fleet['location'] as Map<String, dynamic>;
  final (q, r) = (at['q'] as int, at['r'] as int);
  check(fleet['policy'] == null && fleet['state'] != 'Moving' && fleet['cooldown_remaining'] == 0, 'the fleet is flown by hand and stands ready at [$q,$r]');
  final sectors = ((await browser.last('full_state'))!['known_sectors'] as List).cast<Map<String, dynamic>>();
  final lanes = (sectors.firstWhere((s) => '${s['location']}' == '$at')['connections'] as List).cast<Map<String, dynamic>>();
  final gate = gates.entries.firstWhere((g) => lanes.any((l) => l['q'] == q + g.value.$1 && l['r'] == r + g.value.$2));
  final target = '[${q + gate.value.$1},${r + gate.value.$2}]';
  final errors = await browser.tap('t.errors.length') as int;

  final stop = await browser.find('stop');
  final ticks = await browser.tap('t.counts.tick_update') as int;
  await browser.until('the next tick', () async => (await browser.tap('t.counts.tick_update') as int) > ticks);
  await browser.type(gate.key);
  await browser.clickAt(stop);
  await browser.until('the jump and then the stop on the wire', () => browser.tap('''(() => {
    const move = t.sent.findIndex((m) => m.action === "move" && m.fleet_id === $id);
    return move >= 0 && t.sent.findIndex((m) => m.action === "stop" && m.fleet_id === $id) > move;
  })()'''));
  check(true, 'the ${gate.key.toUpperCase()} gate key sends the jump to $target, and the Stop tile sends the stop while the windshield plays it');
  final said = await browser.until('the engine\'s word on the jump',
      () => browser.tap('t.events.find((e) => e.kind === "Alert" && e.message.startsWith("fleet $id called off its jump"))?.message ?? null')) as String;
  check(RegExp('^fleet $id called off its jump to ${RegExp.escape(target)}: the [0-9.]+ fuel it burned for the jump is forfeit\$').hasMatch(said),
      'the engine abandons the jump and says what it cost: $said');
  check((await browser.tap('t.errors.length')) == errors, 'the stop was taken, not refused');
  await browser.until('the fleet at rest where it was', () async {
    final now = await firstFleet(browser);
    return now['state'] != 'Moving' && '${now['location']}' == '$at';
  });
  await browser.untilText(said);
  check(true, 'the fleet stays at [$q,$r], and the page shows the engine\'s sentence');
  await Future<void>.delayed(const Duration(milliseconds: 600));
  await browser.shot('21a-jump-called-off-1440');
  await Future<void>.delayed(const Duration(seconds: 2));
}

const scoutCost = {'metal': 200, 'crystal': 50, 'deuterium': 30};

/// The stockpile in the newest tick update that carries one.
/// The founding panel, against the engine itself. An outpost is founded by a
/// loaded convoy standing in rings 9 to 20, which is further than this check
/// has the time to fly: it founds nothing. O opens the panel for the starter
/// fleet where it stands, the survey of its sector is asked of the engine in
/// the worker and answered, and the panel shows the engine's reason why no
/// outpost can be founded there, word for word, at desktop width and in a
/// phone's sheet, with Found off. A typed `found` is refused by the engine in
/// the same sentence.
Future<void> founding(Browser browser) async {
  print('founding an outpost');
  final id = (await firstFleet(browser))['id'] as int;
  final errors = await browser.tap('t.errors.length') as int;

  await browser.type('o');
  await browser.untilText('found an outpost');
  await browser.until('the survey on the wire', () => browser.tap('t.sent.some((m) => m.action === "outpost_survey" && m.fleet_id === $id)'));
  final survey = await browser.until('the engine\'s survey', () => browser.last('outpost_survey'));
  final at = survey['sector'] as Map<String, dynamic>;
  final (q, r) = (at['q'] as int, at['r'] as int);
  final ring = [q.abs(), r.abs(), (q + r).abs()].reduce((a, b) => a > b ? a : b);
  final cost = survey['cost'] as Map<String, dynamic>;
  check(
      survey['fleet_id'] == id &&
          '${(await firstFleet(browser))['location']}' == '$at' &&
          survey['ring'] == ring &&
          cost['metal'] == 1000 &&
          cost['crystal'] == 500 &&
          cost['deuterium'] == 200,
      'the engine in the worker surveys the sector the fleet stands in: ring $ring, level 1 for 1000 metal, 500 crystal and 200 deuterium');
  final refusal = survey['refusal'] as String?;
  check(refusal != null && (ring >= 9 || refusal == 'fleet $id is at ring $ring; an outpost stands in rings 9 to 20'),
      'and says why no outpost can be founded there now: $refusal');
  await browser.untilText(refusal!);
  check(true, 'the founding panel shows that sentence, word for word');
  check((await browser.tap('t.errors.length')) == errors, 'a survey is an answer, not an error');
  await browser.shot('21e-founding-refused-1440');
  await browser.resize(390, 844);
  await browser.untilText(refusal);
  await browser.shot('21e-founding-refused-390');
  await browser.resize(1440, 900);

  await browser.click('found outpost');
  await Future<void>.delayed(const Duration(milliseconds: 600));
  check((await browser.tap('t.sent.some((m) => m.action === "found_outpost")')) == false, 'Found is off: a press sends nothing');
  await browser.command('found');
  await browser.until('the typed found on the wire', () => browser.tap('t.sent.some((m) => m.action === "found_outpost" && m.fleet_id === $id)'));
  final refused = await browser.until('the engine\'s refusal', () async {
    final last = await browser.last('error');
    return last != null && last['message'] == refusal ? last : null;
  });
  check(refused['code'] == 'InvalidTarget' || ring >= 9, 'a typed found is refused by the engine in the same sentence (${refused['code']})');
  await browser.until('the refusal on screen', () async => (await browser.toasts()).any((t) => t['text'] == 'Refused. $refusal'));
  check((await browser.last('tick_update'))!['outpost'] == null, 'and no outpost was founded');

  await browser.type('o');
  await browser.until('the founding panel to close', () async => !(await browser.text()).toLowerCase().contains('found an outpost'));
}

Future<Map<String, dynamic>> stockpile(Browser browser) async {
  final player = await browser.until('a tick update with the stockpile', () async => (await browser.last('tick_update'))?['player']);
  return ((player as Map<String, dynamic>)['resources'] as Map<String, dynamic>);
}

/// A ship batch is paid in full when it is ordered. A new empire has 100
/// deuterium and makes none, a Shipyard takes 50 and a scout 30: so with a
/// new Shipyard the order button for a batch of five is off and says what is
/// missing, one scout is ordered and paid for at once, a second is refused
/// by the engine and costs nothing, and a building the stockpile cannot pay
/// for queues and shows its shortfall in its box, at desktop width and at a
/// phone's.
Future<void> shipBatches(Browser browser) async {
  print('ship batches');
  await browser.command('build shipyard');
  await browser.until('the Shipyard to finish',
      () => browser.tap('t.events.some((e) => e.kind === "BuildingCompleted" && e.building_type === "Shipyard")'),
      timeout: const Duration(minutes: 3));
  final young = await stockpile(browser);
  check((young['deuterium'] as num) == 50 && (young['metal'] as num) >= 200 && (young['crystal'] as num) >= 50,
      'the stockpile covers one scout and not two: $young');

  await browser.click('homeworld');
  await browser.untilText('build slot a');
  await browser.press('3', 'Digit3', 51, text: '3');
  await browser.untilText('scout');
  await browser.press('=', 'Equal', 187, text: '=');
  await browser.untilText('short by');
  final wanted = (await browser.text()).toLowerCase();
  check(wanted.contains('build x5') && wanted.contains('short by') && wanted.contains(' de'), 'a batch of five shows as unavailable, with what is missing');
  await browser.shot('03b-batch-short-1440');
  await browser.click('build x5');
  await browser.press('Enter', 'Enter', 13, text: '\r');
  await browser.untilText('cannot queue');
  await Future<void>.delayed(const Duration(milliseconds: 1200));
  check((await browser.tap('t.sent.some((m) => m.action === "build_ship")')) == false,
      'neither its order button nor Enter sends a batch the stockpile does not cover');

  await browser.press('-', 'Minus', 189, text: '-');
  await browser.until('the button for one scout', () async => (await browser.text()).toLowerCase().contains('build x1'));
  await browser.press('Enter', 'Enter', 13, text: '\r');
  await browser.until('one scout ordered on the wire',
      () => browser.tap('t.sent.some((m) => m.action === "build_ship" && m.ship_class === "Scout" && m.count === 1)'));
  final started = jsonDecode(await browser.until('the batch to start',
      () => browser.tap('JSON.stringify(t.events.find((e) => e.kind === "Queue" && e.queue_type === "Ship" && e.action === "Started") ?? null)')
          .then((found) => found == 'null' ? null : found)) as String) as Map<String, dynamic>;
  check(scoutCost.entries.every((e) => (started['paid'][e.key] as num) == e.value), 'one scout was paid for in full when it was ordered: ${started['paid']}');

  await browser.until('the scout to be delivered', () => browser.tap('t.events.some((e) => e.kind === "ShipBuilt" && e.ship_class === "Scout")'));
  await Future<void>.delayed(const Duration(milliseconds: 2500));
  final before = await stockpile(browser);
  check((before['deuterium'] as num) < 30, 'which left less deuterium than a scout costs: $before');
  await browser.command('ship scout');
  final refusal = await browser.until('the refusal on the wire', () => browser.last('error'));
  final message = refusal['message'] as String;
  check(
      refusal['code'] == 'InsufficientResources' &&
          message.startsWith('Scout x1 costs 200 metal, 50 crystal and 30 deuterium; the stockpile is short by ') &&
          message.endsWith(' deuterium. Nothing was ordered: a batch is paid in full when it is ordered.'),
      'the engine refuses a scout the stockpile does not cover and says what is missing: $message');
  await browser.until('the refusal on screen', () async => (await browser.toasts()).any((t) => t['text'] == 'Refused. $message'));
  check(true, 'and the page shows the engine\'s sentence where the player is looking');
  await Future<void>.delayed(const Duration(milliseconds: 600));
  await browser.shot('03f-typed-order-refused');
  await Future<void>.delayed(const Duration(milliseconds: 1900));
  final after = await stockpile(browser);
  check(
      (before['deuterium'] as num) - (after['deuterium'] as num) < 30 &&
          (after['metal'] as num) >= (before['metal'] as num) &&
          (after['crystal'] as num) >= (before['crystal'] as num),
      'nothing was charged for it: $before then $after');
  check((await browser.tap('t.events.filter((e) => e.kind === "Queue" && e.queue_type === "Ship").length')) == 1, 'and nothing was ordered');

  await browser.command('build shipyard');
  await browser.press('1', 'Digit1', 49, text: '1');
  final box = await browser.until('the waiting Shipyard in the building box', () async {
    for (final line in (await browser.text()).split('\n')) {
      if (line.contains('SHIPYARD LV.2') && line.contains('short by')) return line;
    }
    return null;
  });
  check(box.contains(' De'), 'the building box shows the waiting Shipyard and what it is short of: $box');
  await browser.shot('03c-waiting-short-1440');
  await browser.resize(390, 844);
  await browser.shot('03d-waiting-short-390');
  await browser.press('3', 'Digit3', 51, text: '3');
  await browser.untilText('scout');
  await browser.press('=', 'Equal', 187, text: '=');
  await browser.untilText('short by');
  await browser.shot('03e-batch-short-390');
  await browser.resize(1440, 900);
  await browser.press('-', 'Minus', 189, text: '-');
  await browser.click('overview');
  await browser.untilText('admiral');
  check((await browser.tap('t.errors.length === 1 && t.errors[0].code === "InsufficientResources"')) == true,
      'the refused scout is the only error frame so far');
}

/// Every screen and panel a new commander meets, pictured at desktop and at
/// phone width for whoever reads the pictures: the check here is only that
/// each one opens. The fleet scans from home on the way.
Future<void> screens(Browser browser) async {
  print('screens');
  await browser.shotBoth('20-overview');
  await browser.wheel(900);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shot('20b-overview-lower-1440');
  await browser.wheel(-900);

  await browser.type('gw');
  await browser.untilText('fleet status');
  await browser.type('v');
  await browser.until('the scan to come back', () => browser.tap('t.events.some((e) => e.kind === "ScanCompleted")'));
  await Future<void>.delayed(const Duration(seconds: 1));
  await browser.shotBoth('21-windshield');
  for (final (width, height) in [(1100, 800), (1180, 800), (1366, 768)]) {
    await browser.resize(width, height);
    await browser.shot('21b-windshield-$width');
  }
  await browser.resize(1440, 900);

  await browser.click('load hold');
  await browser.untilText('load a hold');
  check(true, 'the windshield\'s Load hold button opens the hold panel on the Storage tab');
  await browser.type('gw');
  await browser.untilText('fleet status');
  await browser.command('load deut');
  await browser.until('the hold laden', () async => (await firstFleet(browser))['laden'] == true);
  await browser.untilText('cargo - laden');
  await browser.shot('21c-windshield-laden-1440');
  await browser.resize(390, 844);
  await browser.click('status');
  await browser.wheel(500, x: 195);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shot('21c-windshield-laden-390');
  await browser.click('status');
  await browser.resize(1440, 900);
  await browser.type('l');
  await browser.untilText('load a hold');
  check(true, 'and so does L, with the hold laden');
  await browser.command('unload');
  await browser.until('the hold empty again', () async => (await firstFleet(browser))['laden'] == false);
  await browser.type('gw');
  await browser.untilText('fleet status');

  await callOffAJump(browser);
  await standingOrders(browser);
  await founding(browser);

  await browser.type('t');
  await browser.untilText('give from the hold');
  await browser.shotBoth('22-windshield-exchange');
  await browser.type('t');
  await browser.until('the exchange panel to close', () async => !(await browser.text()).toLowerCase().contains('give from the hold'));
  await browser.click('manage fleet');
  await browser.untilText('hide fleet');
  await browser.shotBoth('23-windshield-fleet');
  await browser.click('hide fleet');

  await browser.type('gm');
  await browser.untilText('click to plan a route');
  await browser.shotBoth('24-map');

  await browser.type('gh');
  await browser.untilText('build slot a');
  for (final (digit, tab) in [('1', 'buildings'), ('2', 'research'), ('3', 'shipyard'), ('4', 'defence'), ('5', 'storage')]) {
    await browser.type(digit);
    await Future<void>.delayed(const Duration(milliseconds: 400));
    await browser.shotBoth('25-homeworld-$tab');
  }
  await browser.type('1');

  await browser.press('?', 'Slash', 191, text: '?');
  await browser.untilText('key reference');
  await browser.shotBoth('26-keys-and-settings');
  await browser.press('Escape', 'Escape', 27);
  await browser.until('the key reference to close', () async => !(await browser.text()).toLowerCase().contains('key reference'));
  await browser.click('alerts');
  await browser.untilText('esc to close');
  await browser.shotBoth('27-alerts');
  await browser.press('Escape', 'Escape', 27);

  await browser.type('go');
  await browser.untilText('admiral');
  check(true, 'overview, windshield, exchange, fleet, map, the five homeworld tabs, keys and alerts all open, at 1440 and 390 px');

  await browser.command('help');
  await browser.wheel(1600);
  await browser.untilText('h[arvest] [metal|crystal|deut]');
  check(true, 'the log shows the answer to help from its first line');
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shotBoth('28-overview-log-help');
  await browser.wheel(-1600);
}

/// The page as a player on the LAN gets it: over plain http from a name that
/// is not this machine's, which is no secure context. The browser gives such
/// a page no locks, so a second tab is not kept out and finds the save the
/// first one is playing. Both front doors say to play in one tab.
Future<void> plainHttp(Devtools devtools, StaticHost host, Directory shots) async {
  print('a page that is not a secure context');
  final url = 'http://$plainHost:${host.port}/';
  final first = await Browser.open(devtools, shots);
  await first.goto(url);
  final idle = await first.untilStatus('idle', (s) => s['phase'] == 'idle');
  check((await first.eval('window.isSecureContext === false && !("locks" in navigator)')) == true,
      'the page from $url is not a secure context and has no locks');
  check(idle['exclusive'] == false, 'the engine says it cannot keep a second tab out');
  await first.untilText('in one tab');
  check((await first.eval('!("storage" in navigator)')) == true, 'nor can such a page ask the browser to keep its save');
  await first.untilText('may clear the save when the device runs short of storage');
  check(true, 'and the front door says that the browser may clear the save');
  await first.shot('12-plain-http-front-door');
  await first.click('blitz');
  await first.untilText('always at the controls');
  await first.click('start');
  await first.untilStatus('running', (s) => s['phase'] == 'running' && s['world'] != null);

  final second = await Browser.open(devtools, shots);
  await second.goto(url);
  final found = await second.untilStatus('the save or a lock', (s) => s['phase'] == 'held' || s['phase'] == 'locked');
  check(found['phase'] == 'held', 'a second tab is not kept out there: it finds the save the first tab is playing');
  await second.untilText('in one tab');
  check(true, 'and its front door says to play in one tab, as the first one did');
  await second.shot('12b-plain-http-second-tab');
  await second.close();
  await first.close();
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
  check(browser.elsewhere(host).isEmpty, 'the bundle loads with no other host reachable, and asks none for anything (${browser.elsewhere(host)})');
  final door = (await browser.text()).toLowerCase();
  check(!door.contains('in one tab') && !door.contains('clear the save'),
      'from this machine the front door has nothing to say about a second tab or a save the browser may clear');
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
  await browser.command('build metal');
  await browser.until('the build command on the wire',
      () => browser.tap('t.sent.some((m) => m.type === "command" && m.action === "build" && m.building_type === "MetalMine")'));
  await browser.until('the Metal Mine to finish', () => browser.tap(
      't.events.some((e) => e.kind === "BuildingCompleted" && e.building_type === "MetalMine" && e.new_level === 2)'));
  final after = (await browser.last('tick_update'))!;
  check(level(after['homeworld_update'] as Map<String, dynamic>, 'MetalMine') == 2, 'the order was accepted and completed: Metal Mine level 2');
  check((await browser.tap('t.errors.length')) == 0, 'no error frame so far');
  await browser.untilText('tell you when this finishes');
  await Future<void>.delayed(const Duration(milliseconds: 600));
  await browser.shot('03-order-done');
  await browser.click('not now');
  await browser.until('the offer of browser notices to leave',
      () async => !(await browser.text()).toLowerCase().contains('tell you when this finishes'));
  check(true, 'the offer of browser notices outlasts the toasts raised after it and is answered from its own button');

  await toastsStayWhole(browser);

  print('the exchange');
  final hub = (first['world']['hub'] as List).cast<Map<String, dynamic>>();
  check(hub.length == 1 && '${hub.single['location']}' == '{q: 0, r: 0}' && '${hub.single['services']}' == '[exchange]',
      'the world names its hub at the centre, with the exchange');
  check((first['player']['exchange'] as Map)['pressure'] == 0, 'the commander starts with an empty meter');
  await browser.command('quote metal deut 100');
  await browser.until('the quote request on the wire', () => browser.tap(
      't.sent.some((m) => m.action === "exchange_quote" && m.give === "metal" && m.want === "deuterium" && m.amount === 100)'));
  final quote = await browser.until('the engine\'s quote', () => browser.last('exchange_quote'));
  check(quote['amount'] == 100 && (quote['receive'] as num) >= 12 && (quote['receive'] as num) <= 37 && quote['par'] == 50,
      'the engine quotes 100 metal at ${quote['receive']} deuterium, a fee of ${((quote['fee'] as num) * 100).round()}%');
  check('${quote['refusal']}'.contains('from the exchange at the hub'), 'and says the fleet is not at the hub: ${quote['refusal']}');
  check((await browser.tap('t.errors.length')) == 0, 'a quote is an answer, not an error');

  await loadHold(browser);
  await shipBatches(browser);
  await screens(browser);

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
  const kept = {
    'preset': 'prospect',
    'params': {'min_fuel_pct': 10, 'cargo_return_pct': 85, 'max_range': 2, 'engage_ratio_x10': 15, 'fuel_budget_pct': 35},
  };
  await browser.type('gw');
  await browser.untilText('fleet status');
  await browser.type('p');
  await browser.click('doctrine prospect');
  await browser.untilText('least odds to fight');
  for (var i = 0; i < 2; i++) {
    await browser.click('range from home down');
  }
  await browser.click('fuel budget, of income up');
  await browser.click('set prospect');
  await browser.until('the fleet under prospect at two hexes and 35%', () async => jsonEncode((await firstFleet(browser))['policy']) == jsonEncode(kept));
  await browser.press('Escape', 'Escape', 27);
  await browser.type('go');
  await browser.untilText('admiral');
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
  final flownAfter = ((restored['fleets'] as List).first as Map<String, dynamic>)['policy'];
  check(jsonEncode(flownAfter) == jsonEncode(kept), 'the reloaded page reads the fleet\'s orders with their settings off the wire: ${jsonEncode(flownAfter)}');
  await browser.type('gw');
  await browser.untilText('fleet status');
  await browser.type('p');
  await browser.untilText('range 2 hexes - engage 1.5x - fuel 35%');
  check(true, 'and the panel shows the settings in force for orders this page did not give');
  await browser.shot('06b-orders-restored-1440');
  await browser.click('doctrine manual');
  await browser.click('clear orders');
  await browser.until('the fleet flown by hand again', () async => (await firstFleet(browser))['policy'] == null);
  await browser.press('Escape', 'Escape', 27);
  await browser.until('the fleet to finish a jump its orders had begun', () async => (await firstFleet(browser))['state'] != 'Moving');
  await browser.type('go');
  await browser.untilText('admiral');

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
  check((await browser.tap('t.errors.length')) == 0, 'no error frame since the page was last loaded');

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
  check(browser.elsewhere(host).isEmpty, 'in all of that the page asked no other host for anything (${browser.elsewhere(host)})');

  await plainHttp(devtools, host, shots);

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
