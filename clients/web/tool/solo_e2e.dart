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
// the first steps of an empire on OGame's catalogue (a building queued ahead
// of its resources shown with its shortfall, mines short of energy and the
// warning of it, a Solar Satellite built in the yard and counted at home, a
// batch the stockpile does not cover with its order button off, the
// engine's refusal of a typed order shown on screen with nothing charged, a
// defence ordered by typed command), the catalogue in the homeworld's tabs
// and their inspector, every screen pictured at
// both widths, a flight and a fight (Light Fighters built by the batch into
// the docked fleet, two split off by count through the picker and flown
// into a group and out of it again: the flee control with the engine's
// quote, the leaving panel, the parting rounds, the fled ending, the group
// on their trail if it follows, the group's losses and when it mends; then
// the rest flown at the same group, which they outweigh, the round panel
// during the fight and at its end, the damage and the home dock's quote for
// it, a refit waiting, and the mending and the refit at home), the bots on
// the board and acting, a reload that
// restores the
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

  /// The height of the window, which starts as a desktop's.
  int _height = 900;

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
    _height = height;
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

  /// A real click on the control whose label contains [label]. One that a
  /// scrolling column holds below or above the window is wheeled into it
  /// first, as a player would.
  Future<void> click(String label) async {
    var node = await find(label);
    for (var tries = 0; tries < 3 && ((node['y'] as num) < 8 || (node['y'] as num) > _height - 8); tries++) {
      await wheel(((node['y'] as num) - _height / 2).round(), x: (node['x'] as num).round());
      await Future<void>.delayed(const Duration(milliseconds: 400));
      node = await find(label);
    }
    await clickAt(node);
  }

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
    // Chromium's helpers may still be writing their last files when the
    // browser itself has gone: the profile is cleared once they have, and
    // left where it is, with a word, if they never do.
    for (var tries = 0; profile.existsSync(); tries++) {
      try {
        profile.deleteSync(recursive: true);
      } on FileSystemException {
        if (tries == 5) {
          stderr.writeln('the browser profile was left at ${profile.path}');
          break;
        }
        await Future<void>.delayed(const Duration(seconds: 1));
      }
    }
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

/// "1,234,567": a whole number as the page prints an amount.
String grouped(int n) => '$n'.replaceAllMapped(RegExp(r'\B(?=(\d{3})+$)'), (_) => ',');

/// A new empire's two Light Fighters stand at the home dock with an empty
/// hold and the stockpile holds 100 deuterium, which nothing adds to yet.
/// The Storage tab's hold panel loads it, the dock leaves it aboard, the
/// panel says so at desktop width and at a phone's, and a typed `unload`
/// puts it back.
Future<void> loadHold(Browser browser) async {
  print('loading a hold');
  await browser.click('homeworld');
  await browser.untilText('build slot a');
  await browser.press('5', 'Digit5', 53, text: '5');
  await browser.untilText('load a hold');
  final empty = await firstFleet(browser);
  final hold = grouped((empty['cargo_capacity'] as num).floor());
  check(empty['laden'] == false && (empty['cargo']['deuterium'] as num) == 0 && (empty['cargo_capacity'] as num) > 100,
      'the fleet is docked with an empty hold of $hold');
  final ships = [for (final s in empty['ships'] as List) '${(s as Map<String, dynamic>)['count']}x ${s['class']}'];
  check('$ships' == '[2x LightFighter]' && empty['ship_count'] == 2, 'a new empire\'s fleet is two Light Fighters, counted by class');
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
/// shields and it has attacked a group in the home sector). The panel shows
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
  await browser.untilText('leave a fight below');
  for (var i = 0; i < 2; i++) {
    await browser.click('leave a fight below up');
  }
  await browser.until('the steppers at one hex, ten to one and leaving below 1.3', () async {
    final text = await browser.text();
    return text.contains('1 HEX') && text.contains('10.0X') && text.contains('1.3X');
  });
  await browser.shot('21d-orders-seven-settings-1440');
  await browser.click('set patrol home');
  await browser.until('the orders on the wire', () => browser.tap('''t.sent.some((m) => m.type === "policy_update" && m.fleet_id === ${before['id']} &&
      m.preset === "patrol_home" && JSON.stringify(m.params) ===
      JSON.stringify({min_fuel_pct: 10, cargo_return_pct: 85, max_range: 1, engage_ratio_x10: 100, disengage_ratio_x10: 13, fuel_budget_pct: 20, refine: false}))'''));
  check(true, 'Set sends the doctrine with its seven settings, the disengage ratio among them, as the typed policy does');

  Future<String> report(String action) async => await browser.until(
      'the engine\'s "$action" report',
      () => browser.tap('t.events.findLast((e) => e.kind === "PolicyAction" && e.action === "$action")?.reason ?? null')) as String;
  final set = await report('engage');
  check(set == 'doctrine set: patrol home (range 1, engage 10.0x, disengage 1.3x, fuel budget 20% of deuterium income)', 'the engine confirms the orders: $set');
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
            'params': {
              'min_fuel_pct': 10,
              'cargo_return_pct': 85,
              'max_range': 1,
              'engage_ratio_x10': 100,
              'disengage_ratio_x10': 13,
              'fuel_budget_pct': 20,
              'refine': false,
            },
          }),
      'the fleet row carries patrol home with the settings in force: ${jsonEncode(flown)}');
  await browser.untilText('range 1 hex - engage 10.0x - fuel 20%');
  await browser.untilText('leaves a fight below 1.3x');
  check(true, 'and the panel shows them under the doctrine, the ratio it leaves a fight below on a line of its own');
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
/// it. A jump charges for a tick or two, and the engine takes a stop for
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
          ['metal', 'crystal', 'deuterium'].every((k) => (cost[k] as num) > 0),
      'the engine in the worker surveys the sector the fleet stands in: ring $ring, level 1 for ${cost['metal']} metal, '
      '${cost['crystal']} crystal and ${cost['deuterium']} deuterium');
  await browser.until('the founding panel to print that price', () async {
    final text = await browser.text();
    return ['metal', 'crystal', 'deuterium'].every((k) => text.contains(grouped((cost[k] as num).round())));
  });
  check(true, 'and the founding panel prints that price');
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

/// The homeworld in the newest tick update.
Future<Map<String, dynamic>> homeworld(Browser browser) async =>
    (await browser.last('tick_update'))!['homeworld_update'] as Map<String, dynamic>;

/// Four minutes of the world at once: the worker's clock is moved on and the
/// engine plays the ticks it owes. So short a gap is a stalled page to the
/// engine, not an absence, and nothing is said of it.
Future<void> passFourMinutes(Browser browser) async {
  final states = await browser.tap('t.counts.full_state') as int;
  await browser.jumpClock(240 * 1000);
  await browser.until('the world brought up to date', () async => (await browser.tap('t.counts.full_state') as int) > states,
      timeout: const Duration(minutes: 3));
  final tick = (await browser.last('full_state'))!['tick'] as int;
  await browser.until('a tick after it', () async => ((await browser.last('tick_update'))!['tick'] as int) >= tick);
}

/// Let the world run on, four minutes at a time, until [done] holds.
Future<void> passUntil(Browser browser, String what, Future<bool> Function() done) async {
  for (var passes = 0; passes < 15; passes++) {
    if (await done()) return;
    await passFourMinutes(browser);
  }
  if (!await done()) fail('$what did not come about in an hour of the world');
}

Map<String, dynamic> option(Map<String, dynamic> homeworld, String list, String key, String name) =>
    (((homeworld['catalog'] as Map<String, dynamic>)[list] as List).firstWhere((o) => o[key] == name) as Map<String, dynamic>);

/// The first steps of an empire on this catalogue, against the engine
/// itself. A new empire has Metal Mine 1, Crystal Mine 1 and a Solar Plant
/// that just powers them, 100 deuterium and nothing that makes any. So:
///
/// - a Robotics Factory, which costs deuterium the stockpile lacks, queues
///   ahead of its resources and shows its shortfall in its box, at desktop
///   width and at a phone's;
/// - the Metal Mine raised earlier needs more energy than is made: the
///   state says the share the mines run at, and the homeworld and the top
///   bar print it as a warning, until Solar Plant levels cover it;
/// - with a Deuterium Synthesizer, the Robotics Factory at 2 and a Shipyard
///   the yard builds what needs nothing more: a Solar Satellite. A batch of
///   ten the stockpile does not cover has its order button off and says
///   what is missing; one is ordered and paid for at once, is counted at
///   home when built and adds its energy; a batch of a hundred typed is
///   refused by the engine and costs nothing;
/// - a Rocket Launcher is ordered with the typed `defence` command and
///   stands in the home defence.
///
/// The hour or so of the world this takes is passed four minutes at a time.
Future<void> firstSteps(Browser browser) async {
  print('first steps: a waiting building, energy, the yard');
  await browser.click('homeworld');
  await browser.untilText('build slot a');
  await browser.command('build robotics');
  final box = await browser.until('the waiting Robotics Factory in the building box', () async {
    for (final line in (await browser.text()).split('\n')) {
      if (line.contains('ROBOTICS FACTORY LV.1') && line.contains('short by')) return line;
    }
    return null;
  });
  check(box.contains(' De'), 'the building box shows the waiting Robotics Factory and what it is short of: $box');
  await browser.shot('03c-waiting-short-1440');
  await browser.resize(390, 844);
  await browser.shot('03d-waiting-short-390');
  await browser.resize(1440, 900);

  await browser.press('1', 'Digit1', 49, text: '1');
  final short = (await homeworld(browser))['energy'] as Map<String, dynamic>;
  final share = ((short['fraction'] as num) * 100).floor();
  check((short['made'] as int) < (short['needed'] as int) && share < 100,
      'the Metal Mine at level 2 needs more energy than is made: ${short['made']} of ${short['needed']}, so the mines run at $share%');
  await browser.until('the warning on the homeworld', () async {
    final text = (await browser.text()).toUpperCase();
    return text.contains('MINES AT $share%') && text.contains('SHORT OF ENERGY') && text.contains('EVERY MINE MAKES $share% OF ITS OUTPUT');
  });
  check(true, 'the homeworld prints that share and warns that every mine makes only that much');
  await browser.shotBoth('03g-energy-short');

  for (final order in ['build solar', 'build deut', 'build solar']) {
    await browser.command(order);
  }
  await passUntil(browser, 'a Solar Plant at 3, a Deuterium Synthesizer and a Robotics Factory', () async {
    final hw = await homeworld(browser);
    return level(hw, 'SolarPlant') >= 3 && level(hw, 'DeuteriumSynthesizer') >= 1 && level(hw, 'RoboticsFactory') >= 1;
  });
  final covered = (await homeworld(browser))['energy'] as Map<String, dynamic>;
  check((covered['fraction'] as num) == 1 && (covered['made'] as int) >= (covered['needed'] as int),
      'two levels of the Solar Plant cover the mines and the synthesizer: ${covered['made']} made, ${covered['needed']} needed');
  await browser.until('the warning to leave', () async {
    final text = (await browser.text()).toUpperCase();
    return text.contains('MINES AT 100%') && !text.contains('SHORT OF ENERGY');
  });
  check(((await homeworld(browser))['production']['deuterium'] as num) > 0, 'and the synthesizer makes deuterium');

  for (final order in ['build robotics', 'build shipyard']) {
    await browser.command(order);
  }
  await passUntil(browser, 'a Shipyard, and a stockpile that covers a Solar Satellite and a Rocket Launcher', () async {
    final hw = await homeworld(browser);
    if (level(hw, 'Shipyard') < 1) return false;
    final stock = await stockpile(browser);
    final satellite = option(hw, 'ships', 'ship_class', 'SolarSatellite')['unit_cost'] as Map<String, dynamic>;
    final launcher = option(hw, 'defences', 'kind', 'RocketLauncher')['unit_cost'] as Map<String, dynamic>;
    return ['metal', 'crystal', 'deuterium'].every((k) => (stock[k] as num) >= (satellite[k] as num) + (launcher[k] as num));
  });
  final yard = await homeworld(browser);
  final satellite = option(yard, 'ships', 'ship_class', 'SolarSatellite');
  check(satellite['flies'] == false && (satellite['requires'] as List).every((r) => r['met'] == true),
      'with a Shipyard the yard offers the Solar Satellite, which does not fly');
  final unit = satellite['unit_cost'] as Map<String, dynamic>;
  final perSatellite = (yard['energy'] as Map<String, dynamic>)['per_satellite'] as int;
  final madeBefore = (yard['energy'] as Map<String, dynamic>)['made'] as int;

  await browser.press('3', 'Digit3', 51, text: '3');
  await browser.untilText('solar satellite');
  final cards = ((yard['catalog'] as Map<String, dynamic>)['ships'] as List).cast<Map<String, dynamic>>();
  for (var i = 0; i < cards.indexWhere((o) => o['ship_class'] == 'SolarSatellite'); i++) {
    await browser.press('ArrowRight', 'ArrowRight', 39);
  }
  await browser.untilText('stays at home');
  check(RegExp('ENERGY EACH\\s+\\+$perSatellite\\b').hasMatch((await browser.text()).toUpperCase()),
      'the inspector says it stays at home and what each gives: $perSatellite energy');
  await browser.press('=', 'Equal', 187, text: '=');
  await browser.press('=', 'Equal', 187, text: '=');
  await browser.untilText('short by');
  final wanted = (await browser.text()).toLowerCase();
  check(wanted.contains('build x10') && wanted.contains('short by'), 'a batch of ten shows as unavailable, with what is missing');
  await browser.shot('03b-batch-short-1440');
  await browser.press('Enter', 'Enter', 13, text: '\r');
  await browser.untilText('cannot queue');
  await Future<void>.delayed(const Duration(milliseconds: 1200));
  check((await browser.tap('t.sent.some((m) => m.action === "build_ship")')) == false, 'Enter does not send a batch the stockpile does not cover');
  await browser.resize(390, 844);
  await browser.shot('03e-batch-short-390');
  await browser.resize(1440, 900);

  await browser.press('-', 'Minus', 189, text: '-');
  await browser.press('-', 'Minus', 189, text: '-');
  await browser.until('the button for one satellite', () async => RegExp(r'build x1(?!0)').hasMatch((await browser.text()).toLowerCase()));
  await browser.press('Enter', 'Enter', 13, text: '\r');
  await browser.until('one satellite ordered on the wire',
      () => browser.tap('t.sent.some((m) => m.action === "build_ship" && m.ship_class === "SolarSatellite" && m.count === 1)'));
  final started = jsonDecode(await browser.until('the batch to start',
      () => browser.tap('JSON.stringify(t.events.find((e) => e.kind === "Queue" && e.queue_type === "Ship" && e.action === "Started") ?? null)')
          .then((found) => found == 'null' ? null : found)) as String) as Map<String, dynamic>;
  check(unit.entries.every((e) => (started['paid'][e.key] as num) == (e.value as num)), 'one satellite was paid for in full when it was ordered: ${started['paid']}');

  await browser.until('the satellite to be delivered',
      () => browser.tap('t.events.some((e) => e.kind === "ShipBuilt" && e.ship_class === "SolarSatellite")'), timeout: const Duration(minutes: 2));
  await browser.until('it counted at home', () async => (await homeworld(browser))['satellites'] == 1);
  final withOne = await homeworld(browser);
  check(((withOne['energy'] as Map<String, dynamic>)['made'] as int) == madeBefore + perSatellite && (withOne['satellite_power'] as num) > 0,
      'it is counted at the homeworld, adds $perSatellite energy and stands in the home defence');
  check(((await browser.last('tick_update'))!['fleets'] as List).every((f) => (f['ships'] as List).every((s) => s['class'] != 'SolarSatellite')),
      'and it joined no fleet');
  await browser.until('the inspector to count it', () async => RegExp(r'AT HOME\s+1\b').hasMatch((await browser.text()).toUpperCase()));
  await browser.shot('03h-satellite-built-1440');

  await Future<void>.delayed(const Duration(milliseconds: 2500));
  final before = await stockpile(browser);
  await browser.command('ship sat 100');
  final refusal = await browser.until('the refusal on the wire', () => browser.last('error'));
  final message = refusal['message'] as String;
  check(
      refusal['code'] == 'InsufficientResources' &&
          message.startsWith('Solar Satellite x100 costs ') &&
          message.contains('; the stockpile is short by ') &&
          message.endsWith('. Nothing was ordered: a batch is paid in full when it is ordered.'),
      'the engine refuses a batch the stockpile does not cover and says what is missing: $message');
  await browser.until('the refusal on screen', () async => (await browser.toasts()).any((t) => t['text'] == 'Refused. $message'));
  check(true, 'and the page shows the engine\'s sentence where the player is looking');
  await Future<void>.delayed(const Duration(milliseconds: 600));
  await browser.shot('03f-typed-order-refused');
  await Future<void>.delayed(const Duration(milliseconds: 1900));
  final after = await stockpile(browser);
  check(['metal', 'crystal', 'deuterium'].every((k) => (after[k] as num) >= (before[k] as num)), 'nothing was charged for it: $before then $after');
  check((await browser.tap('t.events.filter((e) => e.kind === "Queue" && e.queue_type === "Ship").length')) == 1, 'and nothing was ordered');

  await browser.command('defence rl');
  await browser.until('the typed defence order on the wire',
      () => browser.tap('t.sent.some((m) => m.action === "build_defence" && m.kind === "RocketLauncher" && m.count === 1)'));
  await browser.until('the Rocket Launcher to stand',
      () => browser.tap('t.events.some((e) => e.kind === "DefenceBuilt" && e.defence === "RocketLauncher")'), timeout: const Duration(minutes: 2));
  final defended = await homeworld(browser);
  final launcher = (defended['defences'] as List).firstWhere((d) => d['kind'] == 'RocketLauncher') as Map<String, dynamic>;
  check(launcher['count'] == 1 && (defended['structure_power'] as num) == (launcher['power'] as num),
      'a typed defence order builds a Rocket Launcher, which stands in the home defence at ${launcher['power']}');
  await browser.press('4', 'Digit4', 52, text: '4');
  await browser.until('the defence tab with its three parts', () async {
    final text = (await browser.text()).toUpperCase();
    return text.contains('DOCKED SHIPS ${grouped((defended['docked_power'] as num).round())}') &&
        text.contains('SOLAR SATELLITES ${grouped((defended['satellite_power'] as num).round())}') &&
        text.contains('DEFENCES ${grouped((defended['structure_power'] as num).round())}');
  });
  check(true, 'and the defence tab prints the three parts of it as the engine counts them');
  final kinds = ((defended['catalog'] as Map<String, dynamic>)['defences'] as List).length;
  check(kinds == 8 && RegExp(r'ONE OF A KIND').allMatches((await browser.text()).toUpperCase()).length == 2,
      'it lists the eight defences, the two domes as one of a kind');

  await browser.press('1', 'Digit1', 49, text: '1');
  await browser.click('overview');
  await browser.untilText('admiral');
  check((await browser.tap('t.errors.length === 1 && t.errors[0].code === "InsufficientResources"')) == true,
      'the refused batch is the only error frame so far');
}

/// The research tab draws a node for each research of the catalog the engine
/// in the worker sent, Computer Technology among them, and the fleet says
/// what a jump takes it.
Future<void> everyTechHasANode(Browser browser) async {
  final techs = (((await homeworld(browser))['catalog'] as Map<String, dynamic>)['research'] as List).cast<Map<String, dynamic>>();
  final page = (await browser.text()).toLowerCase();
  final nodes = RegExp(r' level \d+ of \d+, ').allMatches(page).length;
  check(nodes == techs.length, 'the research tab has a node for each of the engine\'s ${techs.length} researches (it draws $nodes)');
  final computer = techs.firstWhere((o) => o['tech'] == 'ComputerTechnology');
  check(page.contains('computer technology level ${computer['level']} of ${computer['max_level']}'), 'Computer Technology is one of them');
  final ticks = (await firstFleet(browser))['jump_ticks'];
  check(ticks == 2, 'the engine says a jump takes the two Light Fighters 2 ticks (it says $ticks)');
}

/// "Light Fighter" for the wire's "LightFighter": the engine's labels are
/// its wire names with the words apart.
String words(Object? wire) => '$wire'.replaceAllMapped(RegExp(r'(?<=[a-z])(?=[A-Z])'), (_) => ' ');

/// The catalogue as the homeworld's tabs and their inspector show it, with
/// what can be checked off the page against the engine's state: a tile for
/// each of the twelve buildings, the largest prices printed whole, a
/// requirement that asks for energy worded as one, a class's rapid fire in
/// both directions as the world's table has it, and a dome that is one of a
/// kind. The card to inspect is reached with the arrow keys.
Future<void> catalogue(Browser browser) async {
  print('the catalogue');
  final hw = await homeworld(browser);
  final catalog = hw['catalog'] as Map<String, dynamic>;
  final rapid = ((await browser.last('full_state'))!['world']['rapid_fire'] as List).cast<Map<String, dynamic>>();
  List<Map<String, dynamic>> list(String name) => (catalog[name] as List).cast<Map<String, dynamic>>();
  Future<void> select(String tab, int index) async {
    await browser.type(tab);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    for (var i = 0; i < index; i++) {
      await browser.press('ArrowRight', 'ArrowRight', 39);
    }
    await Future<void>.delayed(const Duration(milliseconds: 300));
  }

  String price(Map<String, dynamic> cost, String kind) => grouped((cost[kind] as num).round());
  Future<bool> prices(Map<String, dynamic> cost) async {
    final text = await browser.text();
    return ['metal', 'crystal', 'deuterium'].every((k) => text.contains('/ ${price(cost, k)}'));
  }

  await browser.type('1');
  final buildings = list('buildings');
  check(buildings.length == 12, 'the catalogue has twelve buildings');
  await browser.until('a tile for each building at the level the engine gives it', () async {
    final tiles = (await browser.text()).toLowerCase();
    return buildings.every((o) => tiles.contains('${words(o['building_type']).toLowerCase()} level ${o['level']}'));
  });
  check(true, 'the plan has a tile for each of the twelve buildings, at the level the engine gives it');
  final nanite = buildings.indexWhere((o) => o['building_type'] == 'NaniteFactory');
  await select('1', nanite);
  final million = buildings[nanite]['next']['cost'] as Map<String, dynamic>;
  await browser.until('the Nanite Factory\'s price in the inspector', () => prices(million));
  check((million['metal'] as num) >= 1000000, 'a price in the millions is printed whole: ${price(million, 'metal')} metal for the Nanite Factory');
  await browser.shot('25b-building-nanite-1440');

  final research = list('research');
  final graviton = research.indexWhere((o) => o['tech'] == 'GravitonTechnology');
  await select('2', graviton);
  final asks = (research[graviton]['requires'] as List).cast<Map<String, dynamic>>().firstWhere((r) => r['energy'] == true);
  final worded = RegExp('${grouped(asks['need'] as int)} ENERGY MADE\\s+\\(YOU MAKE ${grouped(asks['have'] as int)}\\)');
  await browser.until('the energy Graviton Technology asks for', () async => worded.hasMatch((await browser.text()).toUpperCase()));
  check(true, 'Graviton Technology asks for ${asks['need']} energy made, and the inspector words it so');
  await browser.shot('25c-research-graviton-1440');

  final ships = list('ships');
  final cruiser = ships.indexWhere((o) => o['ship_class'] == 'Cruiser');
  await select('3', cruiser);
  await browser.untilText('rapid fire at');
  final shown = (await browser.text()).toUpperCase();
  final at = [for (final r in rapid) if (r['class'] == 'Cruiser') '${words(r['against']).toUpperCase()} ${r['shots']}'];
  final from = [for (final r in rapid) if (r['against'] == 'Cruiser') '${words(r['class']).toUpperCase()} ${r['shots']}'];
  check(at.isNotEmpty && from.isNotEmpty && shown.contains(at.join(', ')) && shown.contains(from.join(', ')),
      'the Cruiser\'s inspector lists its rapid fire as the world has it: at ${at.join(', ')}; from ${from.join(', ')}');
  final figures = ships[cruiser];
  check(RegExp('HULL\\s+${grouped((figures['hull'] as num).round())}\\b').hasMatch(shown) && RegExp('A JUMP TAKES\\s+${figures['jump_ticks']} TICK').hasMatch(shown),
      'and its figures are the catalogue\'s, with this empire\'s research: hull ${figures['hull']}, a jump in ${figures['jump_ticks']} ticks');
  await browser.shot('25d-shipyard-cruiser-1440');

  final deathstar = ships.indexWhere((o) => o['ship_class'] == 'Deathstar');
  await select('3', deathstar - cruiser);
  final dear = ships[deathstar]['unit_cost'] as Map<String, dynamic>;
  await browser.until('the Deathstar\'s price in the inspector', () => prices(dear));
  check(true, 'the Deathstar\'s price is printed whole: ${price(dear, 'metal')} metal, ${price(dear, 'crystal')} crystal, ${price(dear, 'deuterium')} deuterium');
  await browser.shot('25e-shipyard-deathstar-1440');

  final defences = list('defences');
  final dome = defences.indexWhere((o) => o['kind'] == 'SmallShieldDome');
  await select('4', dome);
  await browser.until('the dome\'s limit in the inspector', () async => RegExp(r'ONE OF A KIND').allMatches((await browser.text()).toUpperCase()).length == 3);
  check(defences[dome]['max'] == 1 && defences[dome]['room'] == 1, 'a dome is one of a kind, and none stands yet');
  await browser.shot('25f-defence-dome-1440');
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
    if (tab == 'research') await everyTechHasANode(browser);
    await browser.shotBoth('25-homeworld-$tab');
  }
  await catalogue(browser);
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

/// The fleet with [id] in the newest tick update; null once it is gone.
Future<Map<String, dynamic>?> fleetWith(Browser browser, int id) async =>
    ((await browser.last('tick_update'))!['fleets'] as List).cast<Map<String, dynamic>>().where((f) => f['id'] == id).firstOrNull;

String spot(Object? hex) => '${(hex as Map)['q']},${hex['r']}';

/// The chart as the engine holds it now, by sector: the typed `refresh`
/// asks for a full state.
Future<Map<String, Map<String, dynamic>>> chart(Browser browser) async {
  final before = await browser.tap('t.counts.full_state') as int;
  await browser.command('refresh');
  await browser.until('a full state', () async => (await browser.tap('t.counts.full_state') as int) > before);
  return {
    for (final s in ((await browser.last('full_state'))!['known_sectors'] as List).cast<Map<String, dynamic>>()) spot(s['location']): s,
  };
}

/// The fewest hops from [from] to [to] over the charted lanes, through
/// sectors with no hostile group in them; null when the chart has no such way.
List<String>? calmWay(Map<String, Map<String, dynamic>> sectors, String from, String to) {
  final back = <String, String>{from: from};
  final queue = [from];
  while (queue.isNotEmpty) {
    final at = queue.removeAt(0);
    if (at == to) {
      final way = <String>[];
      for (var step = to; step != from; step = back[step]!) {
        way.insert(0, step);
      }
      return way;
    }
    for (final lane in (sectors[at]?['connections'] as List? ?? const [])) {
      final next = spot(lane);
      if (back.containsKey(next) || !sectors.containsKey(next)) continue;
      if (next != to && ((sectors[next]!['hostiles'] as List?) ?? const []).isNotEmpty) continue;
      back[next] = at;
      queue.add(next);
    }
  }
  return null;
}

/// Fly fleet [id] one lane with the windshield's gate key, and wait until
/// it stands there and the windshield has played the jump out.
Future<void> jumpTo(Browser browser, int id, String hop) async {
  final fleet = await browser.until('fleet $id ready to jump', () async {
    final f = await fleetWith(browser, id);
    return f != null && f['state'] != 'Moving' && f['cooldown_remaining'] == 0 ? f : null;
  });
  final here = fleet['location'] as Map<String, dynamic>;
  final [q, r] = hop.split(',').map(int.parse).toList();
  final gate = gates.entries.firstWhere((g) => g.value == (q - (here['q'] as int), r - (here['r'] as int)));
  await browser.type(gate.key);
  await browser.until('fleet $id at [$hop]', () async {
    final f = await fleetWith(browser, id);
    if (f == null) fail('fleet $id was lost on the way to [$hop]');
    return spot(f['location']) == hop && f['state'] != 'Moving';
  });
  await Future<void>.delayed(const Duration(milliseconds: 1400));
}

/// At a phone's width, have the windshield's status sheet open or shut. It
/// keeps the state it was last left in, and only an open one prints the
/// fleet status.
Future<void> sheet(Browser browser, {required bool open}) async {
  if ((await browser.text()).toLowerCase().contains('fleet status') == open) return;
  await browser.click('status');
  await browser.until('the status sheet ${open ? 'open' : 'shut'}', () async => (await browser.text()).toLowerCase().contains('fleet status') == open);
}

int research(Map<String, dynamic> homeworld, String tech) =>
    ((homeworld['research'] as List).firstWhere((r) => r['tech'] == tech) as Map<String, dynamic>)['level'] as int;

/// Give the windshield the fleet [id], by the number key of its place in
/// the fleet list.
Future<void> select(Browser browser, int id) async {
  final fleets = ((await browser.last('tick_update'))!['fleets'] as List).cast<Map<String, dynamic>>();
  final place = fleets.indexWhere((f) => f['id'] == id);
  if (place < 0) fail('fleet $id is gone');
  await browser.type('${place + 1}');
  await browser.untilText('f$id / ');
}

/// A scan by the fleet the windshield flies, and the chart as the engine
/// holds it once the scan is back.
Future<Map<String, Map<String, dynamic>>> scanned(Browser browser) async {
  const scans = 't.events.filter((e) => e.kind === "ScanCompleted").length';
  final before = await browser.tap(scans) as int;
  await browser.type('v');
  await browser.until('the scan to come back', () async => (await browser.tap(scans) as int) > before);
  return chart(browser);
}

/// Whether [probe] comes true within [ms]. Nothing fails when it does not:
/// what lasts a tick or two is pictured when it is caught and said to have
/// been missed when it is not.
Future<bool> soon(Future<bool> Function() probe, {required int ms}) async {
  final deadline = DateTime.now().add(Duration(milliseconds: ms));
  while (DateTime.now().isBefore(deadline)) {
    if (await probe()) return true;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  return false;
}

/// "20", "12.5": fuel as the page prints it.
String fuelWords(num fuel) {
  final tenths = (fuel * 10).round() / 10;
  return tenths == tenths.roundToDouble() ? grouped(tenths.round()) : tenths.toStringAsFixed(1);
}

String plural(Object? n, String noun) => '$n $noun${n == 1 ? '' : 's'}';

/// A flight, against the engine itself: the two Light Fighters that were
/// split off jump into the group at [prey], which the six will fight later.
///
/// - In the fight the state quotes the jump out (`flight`): its fuel, its
///   ticks, the rounds under fire, the odds as they stand, who would follow.
///   The flee control prints the quote and every gate is a way out.
/// - The gate key back to [door] sends an ordinary `move`. The fleet is
///   `Moving` with `fleeing`, the page says it is leaving, and the rounds
///   fought meanwhile name it as fleeing and carry no shot of its own.
/// - The fight ends `fled`, with no wreckage, and the page says so where
///   the two land.
/// - If the group follows (`pursued_by`, a `Pursuit` event), the two jump on
///   toward home at once and the banner and the map are pictured. A convoy
///   never follows, and the hunt prefers convoys, so most runs see none.
/// - The chart keeps the group as the two last saw it: its ships against
///   its muster and the tick it will be whole at, which the map's panel for
///   its sector prints. At this pace it mends in seconds.
/// - The two fly [wayHome] with their damage, and the home dock mends them.
///
/// Returns the tick the group was to be whole at when the two last saw it,
/// or null when they did it no lasting harm.
Future<int?> flight(Browser browser,
    {required int wing, required Map<String, dynamic> group, required String prey, required String door, required List<String> wayHome}) async {
  print('  a flight');
  final atPrey = 'e.sector.q + "," + e.sector.r === "$prey"';
  final roundsAtPrey = 't.events.filter((e) => e.kind === "BattleRound" && $atPrey)';
  final endsAtPrey = 't.events.filter((e) => e.kind === "CombatEnded" && e.mine && $atPrey)';
  final rounds = await browser.tap('$roundsAtPrey.length') as int;
  final ends = await browser.tap('$endsAtPrey.length') as int;
  Future<Map<String, dynamic>> two(String when) async => (await fleetWith(browser, wing)) ?? fail('the two were destroyed $when');
  (int, int) hex(String at) {
    final [q, r] = at.split(',').map(int.parse).toList();
    return (q, r);
  }

  String gateFrom(String from, String to) {
    final (a, b) = (hex(from), hex(to));
    return gates.entries.firstWhere((g) => g.value == (b.$1 - a.$1, b.$2 - a.$2)).key;
  }

  await jumpTo(browser, wing, prey);
  var joined = (await two('on arrival'))['state'] == 'InCombat';
  for (var wait = 0; wait < 12 && !joined && group['behavior'] != 'Passive'; wait++) {
    await Future<void>.delayed(const Duration(milliseconds: 500));
    joined = (await two('on arrival'))['state'] == 'InCombat';
  }
  for (var press = 0; press < 3 && !joined; press++) {
    await browser.type('f');
    await Future<void>.delayed(const Duration(milliseconds: 700));
    joined = (await two('as they attacked'))['state'] == 'InCombat';
  }
  check(joined, 'the two are in a fight at [$prey]');

  final fighting = await browser.until('the engine\'s quote for leaving the fight', () async {
    final f = await two('before they could leave');
    return f['flight'] != null ? f : null;
  });
  final quote = fighting['flight'] as Map<String, dynamic>;
  final chase = quote['chase'] as Map<String, dynamic>?;
  check(
      quote['refusal'] == null && quote['ticks'] == (quote['rounds'] as int) + 1 && quote['label'] is String,
      'the state quotes the jump out: ${fuelWords(quote['fuel'] as num)} fuel, gone in ${plural(quote['ticks'], 'tick')}, ${plural(quote['rounds'], 'round')} under fire; '
      'the fight stands at ${(quote['ratio'] as num).toStringAsFixed(2)}, ${quote['label']}; '
      '${chase == null ? 'nothing would follow' : 'the ${chase['label']} would follow ${plural(chase['hexes'], 'hex')}, ${plural(chase['rounds_per_hex'], 'round')} at each'}');
  if (!const ['deadly', 'risky'].contains(quote['label'])) {
    print('  the chart offered no group that outweighs the two: this flight is from a fight they might have won, '
        'and a flight from one they could not, with the group on their trail, is the widget tests\'');
  }
  await browser.untilText('leave the fight');
  final price = '${fuelWords(quote['fuel'] as num)} FUEL, GONE IN ${plural(quote['ticks'], 'TICK').toUpperCase()}, ${plural(quote['rounds'], 'ROUND').toUpperCase()} UNDER FIRE';
  final control = (await browser.text()).toUpperCase();
  check(
      control.contains('A JUMP OUT COSTS') && control.contains(price) && control.contains('AS IT STANDS') && (control.contains('NOTHING FOLLOWS.') || control.contains(' FOLLOWS ')),
      'the flee control prints the quote, the odds as they stand and who follows: $price');
  check(control.contains('LEAVE THE FIGHT BY '), 'and a gate of a fleet in a fight is a way out of it');
  await browser.shot('35-flee-control-1440');
  await soon(() async => (await browser.tap('$roundsAtPrey.length') as int) > rounds, ms: 1500);

  final sent = await browser.tap('t.sent.length') as int;
  await browser.type(gateFrom(prey, door));
  await browser.until('the move out of the fight on the wire', () => browser.tap('t.sent.slice($sent).some((m) => m.action === "move" && m.fleet_id === $wing)'));
  final gone = await browser.until('the two leaving', () async {
    final f = await two('as they left');
    return f['fleeing'] != null || spot(f['location']) == door ? f : null;
  });
  if (gone['fleeing'] case final Map<String, dynamic> leaving) {
    check(gone['state'] == 'Moving' && spot(leaving['to']) == door && gone['flight'] == null,
        'the gate key is flight: the two are Moving to [$door], gone in ${plural(leaving['ticks'], 'tick')} with ${plural(leaving['rounds'], 'round')} more under fire');
    if (await soon(() async => (await browser.text()).toUpperCase().contains('LEAVING THE FIGHT'), ms: 1200)) {
      final page = (await browser.text()).toUpperCase();
      check(page.contains('JUMP IN ') && page.contains('STOP CALLS THE JUMP OFF'), 'the page says they are leaving, counts the jump down, and says what Stop would cost');
      await browser.shot('36-fleeing-1440');
    } else {
      print('  the jump landed before the leaving panel was read: it is not pictured');
    }
  } else {
    print('  the two had landed before their flight was read from the state: the leaving panel is not pictured');
  }

  final ended = jsonDecode(await browser.until('the end of the fight the two left',
      () => browser.tap('JSON.stringify($endsAtPrey[$ends] ?? null)').then((found) => found == 'null' ? null : found),
      timeout: const Duration(seconds: 30)) as String) as Map<String, dynamic>;
  final parting = jsonDecode(await browser.tap(
          'JSON.stringify($roundsAtPrey.slice($rounds).filter((e) => e.fleets.groups.some((g) => g.fleet_id === $wing && g.fleeing)).map((e) => [e.round, e.tick, e.fleets.shots, e.hostiles.shots]))')
      as String) as List;
  check(ended['outcome'] == 'fled' && ended['player_victory'] == false && ended['salvage'] == null,
      'the fight at [$prey] ended fled after ${plural(ended['rounds'], 'round')}, with no wreckage (outcome ${ended['outcome']})');
  check(parting.length == quote['rounds'] && parting.every((r) => (r as List)[2] == 0),
      'the two were fired on for ${plural(parting.length, 'parting round')} and fired no shot in them: (round, tick, their shots, the group\'s) $parting');
  await browser.untilText('fled under fire');
  check(true, 'where they land the page tells the end of the fight they left: FLED UNDER FIRE');

  var landed = await browser.until('the two out of the fight', () async {
    final f = await two('as they landed');
    return spot(f['location']) == door && f['state'] != 'Moving' ? f : null;
  });
  var rest = wayHome;
  if (landed['pursued_by'] case final Map<String, dynamic> hunter) {
    final next = wayHome.firstOrNull;
    if (next != null) await browser.type(gateFrom(door, next));
    final began = jsonDecode(await browser.tap('JSON.stringify(t.events.findLast((e) => e.kind === "Pursuit" && e.fleet_id === $wing) ?? null)') as String) as Map<String, dynamic>?;
    check(began != null && began['mine'] == true && began['group_id'] == hunter['group_id'],
        'the ${hunter['label']} follows: it lands in ${plural(hunter['arrives_in'], 'tick')} from [${spot(hunter['from'])}], '
        '${hunter['catches'] == true ? 'before the two can be gone' : 'after a jump ordered now is gone'}, and follows ${plural(hunter['hexes_left'], 'hex')} more');
    if (await soon(() async => (await browser.text()).toUpperCase().contains('ARRIVES IN '), ms: 1500)) {
      check((await browser.text()).toUpperCase().contains(' LANDS IN '), 'the pursuit stands over the panels, with what follows and when it lands');
      await browser.shot('37-pursuit-1440');
      await browser.type('gm');
      await browser.untilText('click to plan a route');
      await Future<void>.delayed(const Duration(milliseconds: 500));
      await browser.shot('37b-map-pursuit-1440');
      await browser.type('gw');
      await browser.untilText('fleet status');
    } else {
      print('  the pursuit was over before its banner was read: it is not pictured');
    }
    if (next != null) {
      landed = await browser.until('the two a jump further on', () async {
        final f = await two('with the ${hunter['label']} on their trail');
        return spot(f['location']) == next && f['state'] != 'Moving' ? f : null;
      });
      rest = wayHome.skip(1).toList();
    }
  } else {
    print('  nothing follows the two${chase == null ? ', as the quote said' : ': the quote named the ${chase['label']}, which does not come to [$door]'}; '
        'the pursuit banner is pictured from the widget tests');
  }

  int? wholeAt;
  final remembered = ((((await chart(browser))[prey]!)['hostiles'] as List?) ?? const []).cast<Map<String, dynamic>>().where((g) => g['id'] == group['id']).firstOrNull;
  if (remembered?['whole_at'] case final int at) {
    wholeAt = at;
    final now = (await browser.last('tick_update'))!['tick'] as int;
    check(remembered!['muster'] == group['muster'] && (remembered['ship_count'] as int) <= (group['muster'] as int),
        'the group keeps what the fight took: ${remembered['ship_count']} of ${plural(remembered['muster'], 'ship')}, '
        'hull ${[for (final g in remembered['ships'] as List) '${g['hull']} of ${g['hull_max']}'].join(', ')}, whole again at tick $at (it is tick $now)');
    if ((await two('with the group behind them'))['pursued_by'] == null) {
      final here = hex(spot(landed['location'])), there = hex(prey);
      final (dq, dr) = (there.$1 - here.$1, there.$2 - here.$2);
      await browser.type('gm');
      await browser.untilText('click to plan a route');
      await browser.type('f');
      await Future<void>.delayed(const Duration(milliseconds: 600));
      await browser.clickAt({'x': 759 + mapHex * 1.5 * dq, 'y': 478 + mapHex * (sqrt3 / 2 * dq + sqrt3 * dr)});
      if (await soon(() async => (await browser.text()).toUpperCase().contains('WHOLE AGAIN'), ms: 3000)) {
        final panel = (await browser.text()).toUpperCase();
        final ships = remembered['ship_count'] == remembered['muster'] ? plural(remembered['ship_count'], 'SHIP') : '${remembered['ship_count']} OF ${plural(remembered['muster'], 'SHIP')}';
        check(panel.contains('${words(group['behavior']).toUpperCase()} - ${ships.toUpperCase()}') && panel.contains('A JUMP TAKES IT '),
            'the map\'s panel for [$prey] lists the group against its muster, when it is whole again, how far it follows and its jump');
        await browser.shot('39-wounded-group-1440');
        print('  a phone\'s map keeps the route in its sheet, not the sector: a wounded group at 390 px is pictured from the widget tests, in the contacts');
      } else {
        print('  the map\'s panel for [$prey] did not show the wounded group: it is pictured from the widget tests');
      }
      await browser.press('Escape', 'Escape', 27);
      await browser.press('Escape', 'Escape', 27);
      await browser.type('gw');
      await browser.untilText('fleet status');
    } else {
      print('  the group is still on the trail of the two, who fly on: its panel on the map is pictured from the widget tests');
    }
  } else {
    print('  the two did the group no harm that lasts: a wounded group\'s panel is pictured from the widget tests');
  }

  final worn = await two('after the fight');
  final damage = worn['repair'] as Map<String, dynamic>?;
  print('  the two fly home ${damage == null ? 'whole' : 'with ${plural(damage['ships'], 'damaged ship')}, hull ${worn['hull']} of ${worn['hull_max']}'}, ${worn['ship_count']} of 2 left');
  for (final hop in rest) {
    await jumpTo(browser, wing, hop);
  }
  if (damage != null) {
    final mended = await browser.until('the home dock\'s word on mending the two',
        () => browser.tap('t.events.find((e) => e.kind === "Alert" && e.message.startsWith("fleet $wing was mended at the home dock"))?.message ?? null')) as String;
    check((await two('at the dock'))['repair'] == null, 'home, the dock mends what the flight cost: "$mended"');
  }
  if ((await browser.text()).toUpperCase().contains('FLED UNDER FIRE')) {
    await browser.shot('38-fled-ending-1440');
    await browser.resize(390, 844);
    await sheet(browser, open: true);
    await soon(() async => (await browser.toasts()).isEmpty, ms: 9000);
    await browser.shot('38-fled-ending-390');
    await browser.resize(1440, 900);
  } else {
    print('  the report of the fight had been put away before the two were home: its fled ending is pictured from the widget tests');
  }
  return wholeAt;
}

/// The side of a hex on the map at its closest zoom, which F brings up
/// around the fleet: `mapU` of the client, times the zoom of `centerOn`.
const mapHex = 26 * 1.5;
const sqrt3 = 1.7320508075688772;

/// A fight, against the engine itself, from the yard to the dock and back.
///
/// - A Research Lab, Energy Technology and the Combustion Drive open the
///   Light Fighter; six are ordered as one batch and are built into the
///   fleet docked at home, which then counts eight of them, on the wire and
///   on the page.
/// - The fleet panel's picker takes two of them by count: the wire carries
///   `split` with a class and a count and no ship id, and the engine names
///   the new fleet.
/// - The two that were split off scout by the gate keys for the nearest
///   group the chart shows that the six outweigh about two to one, one of
///   two ships or more before a single ship (which falls in one round) and a
///   passive one before another. They fly into it and out again ([flight]),
///   and go home.
/// - The six fly the same way, wait a jump from the group until a scan
///   shows it in its sector and whole (a group the two hurt keeps its losses
///   and mends, in seconds at this pace; one that went after them comes
///   back), and jump in. A group that does not attack is read on the map's
///   panel for its sector and on the windshield, listed by class, before F
///   fires on it.
/// - The fight is a round a tick. The round panel is read while it lasts
///   and when it has ended: the outcome, the rounds and the losses are the
///   engine's `CombatEnded`.
/// - The fleet carries its damage: the state quotes what the home dock
///   would mend, and the page prints it. Armour Technology is researched
///   while it is away, so a refit waits for it at home, and the page says
///   that too.
/// - Home, the dock mends and refits it and says so, and what is left of
///   the two is merged back through the panel.
///
/// So small a fight lasts two or three seconds, so the round under way is
/// pictured as it stands on a desktop only; a phone's picture is of the
/// last round, opened after the end.
Future<void> fight(Browser browser, Map<String, dynamic> homeHex) async {
  print('a fight');
  final home = spot(homeHex);
  final main = (await firstFleet(browser))['id'] as int;
  await browser.type('gh');
  await browser.untilText('build slot a');
  await browser.command('build lab');
  await passUntil(browser, 'a Research Lab', () async => level(await homeworld(browser), 'ResearchLab') >= 1);
  await browser.command('build lab');
  await browser.command('research energy');
  await passUntil(browser, 'Energy Technology', () async => research(await homeworld(browser), 'EnergyTechnology') >= 1);
  await browser.command('research combustion');
  const batch = 6, all = batch + 2;
  await passUntil(browser, 'the Combustion Drive, a second level of the lab and the price of $batch Light Fighters', () async {
    final hw = await homeworld(browser);
    if (research(hw, 'CombustionDrive') < 1 || level(hw, 'ResearchLab') < 2) return false;
    final fighter = option(hw, 'ships', 'ship_class', 'LightFighter');
    final (stock, cost) = (await stockpile(browser), fighter['unit_cost'] as Map<String, dynamic>);
    return (fighter['requires'] as List).every((r) => r['met'] == true) &&
        ['metal', 'crystal', 'deuterium'].every((k) => (stock[k] as num) >= batch * (cost[k] as num) + 1500);
  });
  await browser.command('ship lf $batch');
  await browser.until('$batch Light Fighters ordered as one batch',
      () => browser.tap('t.sent.some((m) => m.action === "build_ship" && m.ship_class === "LightFighter" && m.count === $batch)'));
  await passUntil(browser, 'the $batch to be built', () async => (await fleetWith(browser, main))?['ship_count'] == all);
  final built = (await fleetWith(browser, main))!;
  final row = (built['ships'] as List).single as Map<String, dynamic>;
  check(
      row['class'] == 'LightFighter' && row['count'] == all && row['damaged'] == 0 && row['hull'] == row['hull_max'] && !row.containsKey('id'),
      'the $batch were built into the docked fleet, which is a count: $all Light Fighters, hull ${row['hull']} of ${row['hull_max']}, no ship with an id');
  final hw = await homeworld(browser);
  check(hw['docked_ship_count'] == all && ((await browser.last('tick_update'))!['player'] as Map)['ship_count'] == all,
      'the homeworld counts $all docked ships and the empire $all in all');

  await browser.type('gw');
  await browser.untilText('f$main / $all ships');
  check((await browser.text()).toUpperCase().contains('LIGHT FIGHTER'), 'the fleet status lists the fleet by class and says $all ships');
  await browser.click('manage fleet');
  await browser.untilText('whole first');
  // The picks live in the panel, and a phone's sheet that is shut holds no
  // panel: they are made again wherever the panel has been opened anew.
  Future<void> pickTwo() async {
    for (var picked = 0; picked < 2 && !(await browser.text()).toLowerCase().contains('split off 2'); picked++) {
      await browser.click('split off more light fighter');
      await Future<void>.delayed(const Duration(milliseconds: 300));
    }
    await browser.untilText('split off 2');
  }

  await pickTwo();
  await browser.shot('23b-split-picker-1440');
  await browser.resize(390, 844);
  await sheet(browser, open: true);
  await browser.untilText('fleet management');
  await browser.wheel(900, x: 195);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await pickTwo();
  await browser.shot('23b-split-picker-390');
  await browser.resize(1440, 900);
  await pickTwo();
  await browser.click('split off 2');
  await browser.until('the split on the wire, by class and count', () => browser.tap('''t.sent.some((m) => m.action === "split" && m.fleet_id === $main &&
      JSON.stringify(m.ships) === '[{"class":"LightFighter","count":2}]' && m.damaged_first === false && !("ship_ids" in m))'''));
  final said = await browser.until('the engine\'s word on the split',
      () => browser.tap('t.events.find((e) => e.kind === "Alert" && e.message.startsWith("fleet $main split: 2 ship(s) now fly as fleet "))?.message ?? null')) as String;
  final wing = int.parse(said.split(' ').last);
  await browser.until('the two fleets', () async => (await fleetWith(browser, main))?['ship_count'] == batch && (await fleetWith(browser, wing))?['ship_count'] == 2);
  check(true, 'the picker splits two off by count: the wire names a class and a count, and the engine answers "$said"');
  await browser.click('hide fleet');

  print('  the hunt, flown by the two that were split off');
  await select(browser, wing);
  var prey = '';
  final flown = <String>[];
  for (var step = 0; prey.isEmpty; step++) {
    if (step == 8) fail('no group the six outweigh two to one within eight scans of home');
    if (step > 0) await Future<void>.delayed(const Duration(seconds: 4));
    final sectors = await scanned(browser);
    final six = (await fleetWith(browser, main))!, two = (await fleetWith(browser, wing))!;
    final here = spot(two['location']);
    final ways = [
      for (final s in sectors.values)
        if (((s['hostiles'] as List?) ?? const []).isNotEmpty && ((s['threat'] as Map)['est_power'] as num) * 1.9 <= (six['power'] as num))
          (
            calmWay(sectors, here, spot(s['location'])),
            ((s['hostiles'] as List).first['ship_count'] as int >= 2 ? 0 : 2) + ((s['hostiles'] as List).first['behavior'] == 'Passive' ? 0 : 1),
            ((s['threat'] as Map)['est_power'] as num) > (two['power'] as num) ? 0 : 1,
          ),
    ].where((w) => w.$1 != null && w.$1!.isNotEmpty).toList()
      ..sort((a, b) => a.$2 != b.$2
          ? a.$2.compareTo(b.$2)
          : a.$3 != b.$3
              ? a.$3.compareTo(b.$3)
              : a.$1!.length.compareTo(b.$1!.length));
    // A single ship falls in one round, before the round panel can be read:
    // the first three scans look for a group of two or more, further out.
    // Of two groups alike in that, the one that outweighs the two is taken,
    // so that their flight is from a fight they could not win.
    final near = ways.where((w) => w.$1!.length <= 4 && (w.$2 < 2 || step >= 3)).firstOrNull;
    if (near != null) {
      final way = near.$1!;
      for (final hop in way.take(way.length - 1)) {
        await jumpTo(browser, wing, hop);
        flown.add(hop);
      }
      prey = way.last;
    } else {
      int ring(String at) {
        final [q, r] = at.split(',').map(int.parse).toList();
        return (q.abs() + r.abs() + (q + r).abs()) ~/ 2;
      }

      final calm = [
        for (final lane in sectors[here]!['connections'] as List)
          if (sectors.containsKey(spot(lane)) && ((sectors[spot(lane)]!['hostiles'] as List?) ?? const []).isEmpty && !flown.contains(spot(lane)) && spot(lane) != home)
            spot(lane),
      ]..sort((a, b) => ring(b).compareTo(ring(a)));
      if (calm.isEmpty) fail('no calm way on from [$here]');
      await jumpTo(browser, wing, calm.first);
      flown.add(calm.first);
    }
  }
  final spotted = (await chart(browser))[prey]!;
  final mustered = (spotted['hostiles'] as List).first as Map<String, dynamic>;
  print('  prey at [$prey]: ${[for (final g in mustered['ships'] as List) '${g['count']}x ${g['class']}'].join(', ')} (${mustered['behavior']}), '
      'power ${(spotted['threat'] as Map)['est_power']} against the six\'s ${(await fleetWith(browser, main))!['power']} '
      'and the two\'s ${(await fleetWith(browser, wing))!['power']}');
  check(
      (mustered['ships'] as List).every((g) => g['count'] is int && g['hull'] == g['hull_max'] && g['unit_hull'] is int) &&
          mustered['ship_count'] == mustered['muster'] &&
          mustered['whole_at'] == null &&
          mustered['jump_ticks'] is int &&
          mustered['chase_hexes'] is int,
      'the group in sight is listed by class with its count, its hull and what one ship is; it is whole, '
      'jumps in ${mustered['jump_ticks']} ticks and follows ${mustered['chase_hexes']} hex(es)');

  final atPrey = 'e.sector.q + "," + e.sector.r === "$prey"';
  final roundsAtPrey = 't.events.filter((e) => e.kind === "BattleRound" && $atPrey)';
  final endsAtPrey = 't.events.filter((e) => e.kind === "CombatEnded" && e.mine && $atPrey)';
  final wholeAt = await flight(browser,
      wing: wing, group: mustered, prey: prey, door: flown.isEmpty ? home : flown.last, wayHome: [...flown.reversed.skip(1), if (flown.isNotEmpty) home]);

  print('  the fight');
  await select(browser, main);
  for (final hop in flown) {
    await jumpTo(browser, main, hop);
  }
  // The six stand a jump from the group. It may be out after the two, and
  // what the chart holds of its sector is what the two last saw: a scan
  // tells where it is and how it stands now.
  Map<String, dynamic>? sighted;
  for (var look = 0; sighted == null; look++) {
    if (look == 12) fail('the group of [$prey] was not back in its sector a minute after the two left it');
    if (look > 0) await Future<void>.delayed(const Duration(seconds: 5));
    final seen = (await scanned(browser))[prey]!;
    final there = ((seen['hostiles'] as List?) ?? const []).cast<Map<String, dynamic>>();
    if (there.length == 1 && there.single['id'] == mustered['id'] && there.single['errand'] == null && there.single['whole_at'] == null) sighted = seen;
    if (look == 0 && there.length == 1 && wholeAt != null) {
      final now = (await browser.last('tick_update'))!['tick'] as int;
      final hurt = there.single['whole_at'] != null;
      check(hurt || now >= wholeAt,
          'when the six arrive at tick $now the group is ${hurt ? 'still hurt, ${there.single['ship_count']} of ${there.single['muster']} ships, whole at tick ${there.single['whole_at']}' : 'whole again'}: '
          'the two had read that it would not be whole before tick $wholeAt');
    }
  }
  final group = (sighted['hostiles'] as List).single as Map<String, dynamic>;
  check(group['ship_count'] == group['muster'] && (group['ships'] as List).every((g) => g['hull'] == g['hull_max']),
      'the six wait until it has mended: ${group['ship_count']} of ${group['muster']} ships, every hull whole');
  final rounds = await browser.tap('$roundsAtPrey.length') as int;
  final ends = await browser.tap('$endsAtPrey.length') as int;
  await jumpTo(browser, main, prey);
  var joined = (await fleetWith(browser, main))!['state'] == 'InCombat';
  // A group that attacks by itself does so within a few ticks, and at this
  // pace its lair stands again within the half minute the map would take.
  for (var wait = 0; wait < 12 && !joined && group['behavior'] != 'Passive'; wait++) {
    await Future<void>.delayed(const Duration(milliseconds: 500));
    joined = (await browser.tap('$roundsAtPrey.length') as int) > rounds;
  }
  if (!joined) {
    await browser.type('gm');
    await browser.untilText('click to plan a route');
    await browser.type('f');
    await Future<void>.delayed(const Duration(milliseconds: 600));
    await browser.clickAt({'x': 759, 'y': 478});
    await browser.untilText('hostile group');
    final panel = (await browser.text()).toUpperCase();
    check(panel.contains('${words(group['behavior']).toUpperCase()} - ${group['ship_count']} SHIP') && panel.contains('F$main HERE'),
        'the map\'s panel for the sector lists the group by class and the fleet standing in it');
    await browser.shotBoth('29-map-hostile-group');
    await browser.press('Escape', 'Escape', 27);
    await browser.press('Escape', 'Escape', 27);
    await browser.type('gw');
    await browser.untilText('fleet status');
    await browser.untilText('hostile group');
    await browser.shotBoth('29b-windshield-hostile-group');
    for (var press = 0; press < 3 && !joined; press++) {
      await browser.type('f');
      await Future<void>.delayed(const Duration(milliseconds: 700));
      joined = await browser.tap('t.sent.some((m) => m.action === "attack" && m.fleet_id === $main)') == true;
    }
    check(joined, 'F fires on the group, a second press where the odds were not in yet');
  } else {
    print('  the group attacked on arrival: its panel on the map is not pictured');
  }

  await browser.until('the first round', () async => (await browser.tap('$roundsAtPrey.length') as int) > rounds);
  final live = await browser.until('the round panel', () async {
    final page = (await browser.text()).toUpperCase();
    return page.contains('LOST SO FAR') || page.contains('ENDED - ') ? page : null;
  });
  await browser.shot('30-fight-round-1440');
  if (live.contains('LOST SO FAR')) {
    check(live.contains('BATTLE') && live.contains('STANDING') && live.contains('FLEETS FIRED') && live.contains('LIGHT FIGHTER > '),
        'while the fight lasts the round panel says what stands on each side and who fired at what');
  } else {
    print('  the fight was over before its panel was read: the round under way is not pictured');
  }
  final ended = jsonDecode(await browser.until('the end of the fight',
      () => browser.tap('JSON.stringify($endsAtPrey[$ends] ?? null)').then((found) => found == 'null' ? null : found),
      timeout: const Duration(seconds: 90)) as String) as Map<String, dynamic>;
  final fought = jsonDecode(await browser.tap('JSON.stringify($roundsAtPrey.slice($rounds).filter((e) => e.tick <= ${ended['tick']}))') as String) as List;
  final elsewhere = await browser.tap(
      'JSON.stringify(t.events.filter((e) => (e.kind === "BattleRound" || e.kind === "CombatEnded") && !($atPrey)).map((e) => [e.kind, e.sector.q, e.sector.r, e.tick, e.round ?? e.rounds, e.mine]))') as String;
  if (elsewhere != '[]') print('  fights seen elsewhere in this run (kind, q, r, tick, round, mine): $elsewhere');
  check(ended['outcome'] == 'hostiles_destroyed' && ended['player_victory'] == true && ended['rounds'] == fought.length,
      'the fight at [$prey] ended with the hostiles destroyed after ${ended['rounds']} rounds, one BattleRound a round '
      '(outcome ${ended['outcome']}, rounds ${[for (final r in fought) (r as Map)['round']]} on ticks ${[for (final r in fought) (r as Map)['tick']]})');
  final first = fought.first as Map<String, dynamic>;
  check((first['fleets']['volleys'] as List).isNotEmpty && first['fleets']['standing'] is int && !jsonEncode(first).contains('ship_id'),
      'a round is by class: ${(first['fleets']['volleys'] as List).length} volley(s) of ours, ${first['fleets']['shots']} shots, and no ship is named');
  await browser.untilText('hostiles destroyed');
  final told = (await browser.text()).toUpperCase();
  check(told.contains('ENDED - ${ended['rounds']} ROUND') && told.contains(' LOST ${group['ship_count']}X '),
      'the panel says how it ended, after how many rounds, and that the group lost all ${group['ship_count']} of its ships');
  await browser.shot('31-fight-end-1440');
  await browser.click('show last round');
  await browser.untilText('hostiles fired');
  await browser.shot('31b-fight-last-round-1440');
  await browser.resize(390, 844);
  await sheet(browser, open: false);
  await browser.shot('31-fight-end-strip-390');
  await sheet(browser, open: true);
  await browser.shot('31-fight-end-390');
  if (!(await browser.text()).toLowerCase().contains('hostiles fired')) await browser.click('show last round');
  await browser.untilText('hostiles fired');
  await browser.shot('31b-fight-last-round-390');
  await browser.resize(1440, 900);
  await browser.click('dismiss');
  await browser.until('the report put away', () async => !(await browser.text()).toUpperCase().contains('ENDED - '));
  check(true, 'Dismiss puts the report of the fight away');

  final worn = await browser.until('the fleet with its damage', () async {
    final f = await fleetWith(browser, main);
    return f != null && f['state'] != 'InCombat' ? f : null;
  });
  final quote = worn['repair'] as Map<String, dynamic>?;
  check(quote != null && (worn['hull'] as int) < (worn['hull_max'] as int) && (quote['home'] as Map)['ships'] == quote['ships'] && (quote['home'] as Map)['price'] == 0,
      'the fleet carries its damage out of the fight (hull ${worn['hull']} of ${worn['hull_max']}), and the state quotes the home dock: all ${quote?['ships']} for nothing');
  final dock = 'HOME DOCK MENDS ALL ${quote!['ships']} (${grouped(quote['hull'] as int)} HULL) FOR NOTHING';
  await browser.untilText(dock);
  check((await browser.text()).toUpperCase().contains('${quote['ships']} DAMAGED SHIP'), 'the fleet status prints the damage and the quote: $dock');
  await browser.command('research armour');
  await browser.until('Armour Technology ordered', () => browser.tap('t.sent.some((m) => m.action === "research" && m.tech === "ArmourTechnology")'));
  await browser.until('Armour Technology researched while the fleet is away', () async => (await fleetWith(browser, main))?['refit_due'] == true,
      timeout: const Duration(seconds: 120));
  await browser.untilText('a refit waits at the home dock');
  final docked = await browser.until('the dock to refit the wing that is home', () async {
    final f = await fleetWith(browser, wing);
    return f != null && f['refit_due'] == false && (f['refit'] as Map)['armour'] == 1 ? f : null;
  });
  final away = (await fleetWith(browser, main))!;
  check((away['refit'] as Map)['armour'] == 0 && away['refit_due'] == true && (docked['refit'] as Map)['armour'] == 1,
      'the fleet in the field fights with Armour 0 and a refit waits for it; the wing at the dock has Armour 1 already');
  await browser.shot('32-damaged-refit-waiting-1440');
  await browser.resize(390, 844);
  await sheet(browser, open: true);
  await browser.wheel(480, x: 195);
  await Future<void>.delayed(const Duration(milliseconds: 500));
  await browser.shot('32-damaged-refit-waiting-390');
  await browser.wheel(-480, x: 195);
  await browser.resize(1440, 900);

  for (final hop in [...flown.reversed, home]) {
    await jumpTo(browser, main, hop);
  }
  final mended = await browser.until('the home dock\'s word on the mending',
      () => browser.tap('t.events.find((e) => e.kind === "Alert" && e.message.startsWith("fleet $main was mended at the home dock"))?.message ?? null')) as String;
  final refitted = await browser.until('and on the refit',
      () => browser.tap('t.events.find((e) => e.kind === "Alert" && e.message.startsWith("fleet $main was refitted at the home dock"))?.message ?? null')) as String;
  final whole = await browser.until('the fleet whole again', () async {
    final f = await fleetWith(browser, main);
    return f != null && f['repair'] == null && f['refit_due'] == false ? f : null;
  });
  check(whole['hull'] == whole['hull_max'] && (whole['refit'] as Map)['armour'] == 1, 'home, the dock mends it and refits it: "$mended"; "$refitted"');
  await browser.untilText('w0 s0 a1');
  check(!(await browser.text()).toLowerCase().contains('a refit waits'), 'and the page shows the levels it now fights with and no damage');
  await browser.shotBoth('33-home-mended-refitted');

  final back = (await fleetWith(browser, wing))!['ship_count'] as int;
  await browser.click('manage fleet');
  await browser.click('merge f$wing in');
  await browser.until('the merge on the wire', () => browser.tap('t.sent.some((m) => m.action === "merge" && m.fleet_id === $main && m.other_fleet_id === $wing)'));
  await browser.until('one fleet again', () async => ((await browser.last('tick_update'))!['fleets'] as List).length == 1);
  check((await fleetWith(browser, main))!['ship_count'] == (whole['ship_count'] as int) + back,
      'the wing that fled, $back of its two left, is merged back through the panel');
  await browser.click('hide fleet');
  await browser.type('go');
  await browser.untilText('admiral');
  await browser.shotBoth('34-overview-after-the-fight');
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
  final start = first['homeworld'] as Map<String, dynamic>;
  check(['MetalMine', 'CrystalMine', 'SolarPlant'].every((b) => level(start, b) == 1), 'the homeworld starts with a Metal Mine, a Crystal Mine and a Solar Plant at level 1');
  final power = start['energy'] as Map<String, dynamic>;
  check(power['made'] == power['needed'] && power['fraction'] == 1, 'which makes just the energy the two mines need (${power['made']})');
  final catalog = start['catalog'] as Map<String, dynamic>;
  check(
      '${[for (final list in ['buildings', 'research', 'ships', 'defences']) (catalog[list] as List).length]}' == '[12, 14, 13, 8]' &&
          (first['world']['rapid_fire'] as List).isNotEmpty,
      'the catalogue is twelve buildings, fourteen researches, thirteen ship classes and eight defences, and the world carries the rapid-fire table');
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
  await firstSteps(browser);
  await screens(browser);
  await fight(browser, home as Map<String, dynamic>);

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
    'params': {
      'min_fuel_pct': 10,
      'cargo_return_pct': 85,
      'max_range': 2,
      'engage_ratio_x10': 15,
      'disengage_ratio_x10': 9,
      'fuel_budget_pct': 35,
      'refine': false,
    },
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
