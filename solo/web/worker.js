// The single-player engine's worker: iac_solo.wasm, the wall clock that
// drives it, and its save in IndexedDB. The page never waits on a tick or a
// catch-up, it only exchanges messages, each {type, text}:
//
//   page -> worker
//     new_game      text: JSON {name, pace, bots}, bots as the server's
//                   --bot specs
//     continue      start the clock of the loaded save
//     take_command  stop catching up, play from here
//     abandon       drop the world and its save
//     open, close   a client session begins, ends
//     frame         text: one client frame (wire JSON)
//     save          save now (the page is being hidden)
//   worker -> page
//     frame         text: one server frame (wire JSON)
//     status        text: JSON {phase, world, options, refused, error,
//                   warning, rate, tick_ms, tick_ms_max}
//       phase    loading | idle (no world) | held (a save is loaded, its
//                clock not started) | running | locked (another tab has the
//                world) | failed
//       error    why the last request was not carried out, or why the
//                engine stopped
//       warning  saves are not being written, and why
//       rate     ticks per second while catching up
//       tick_ms, tick_ms_max  what a live tick cost since the last status:
//                the mean and the worst
//
// Everything else is in the wasm (solo/src): this file decides nothing about
// the game. Requests and replies cross as bytes, a line of JSON and a body.

const SAVE_DB = 'iac-solo';
const SAVE_STORE = 'saves';
const SAVE_KEY = 'world';
const LOCK = 'iac-solo-world';
const LOCK_WAIT_MS = 2000;
const PUMP_MS = 200;
const SLICE_MS = 30;
const STATUS_MS = 200;
const HEARTBEAT_MS = 10000;
const MAX_CHUNK = 20000;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let wasm = null;
let db = null;
let phase = 'loading';
let options = null;
let last = { world: null, refused: null };
let error = null;
let warning = null;
let panic = null;
let rate = 0;
let live = { ticks: 0, ms: 0, worst: 0 };
let chunk = 50;
let pumping = false;
let saving = Promise.resolve();
let statusAt = 0;

function call(head, body) {
  const line = encoder.encode(JSON.stringify({ now_ms: Date.now(), ...head }));
  const length = line.length + (body ? body.length + 1 : 0);
  const at = wasm.iac_request(length);
  const request = new Uint8Array(wasm.memory.buffer, at, length);
  request.set(line);
  if (body) {
    request[line.length] = 10;
    request.set(body, line.length + 1);
  }
  const size = wasm.iac_call();
  const bytes = new Uint8Array(wasm.memory.buffer, wasm.iac_reply(), size);
  const split = bytes.indexOf(10);
  const reply = JSON.parse(decoder.decode(bytes.subarray(0, split)));
  last = { world: reply.world, refused: reply.refused ?? null };
  return { reply, body: bytes.subarray(split + 1) };
}

function sendFrames(body) {
  if (body.length === 0) return;
  for (const text of decoder.decode(body).split('\n')) {
    if (text) postMessage({ type: 'frame', text });
  }
}

function sendStatus() {
  statusAt = performance.now();
  const status = {
    phase,
    options,
    world: last.world,
    refused: last.refused,
    error,
    warning,
    rate,
    tick_ms: live.ticks === 0 ? 0 : live.ms / live.ticks,
    tick_ms_max: live.worst,
  };
  live = { ticks: 0, ms: 0, worst: 0 };
  postMessage({ type: 'status', text: JSON.stringify(status) });
}

function fail(e) {
  phase = 'failed';
  error = panic ?? String(e?.message ?? e);
  sendStatus();
}

function store(mode, run) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(SAVE_STORE, mode);
    const asked = run(tx.objectStore(SAVE_STORE));
    tx.oncomplete = () => resolve(asked.result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

function openDb() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(SAVE_DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(SAVE_STORE);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

async function throughStream(bytes, transform) {
  return new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(transform)).arrayBuffer());
}

// A save document is JSON and shrinks about tenfold.
async function pack(document) {
  if (typeof CompressionStream === 'undefined') return { document };
  return { document: await throughStream(document, new CompressionStream('gzip')), encoding: 'gzip' };
}

function unpack(saved) {
  if (saved.encoding !== 'gzip') return saved.document;
  return throughStream(saved.document, new DecompressionStream('gzip'));
}

// One put in one transaction: a tab killed mid-write keeps the save before it.
function save() {
  saving = saving.then(async () => {
    if (!last.world || phase === 'failed') return;
    const document = call({ op: 'save' }).body.slice();
    if (!db) return;
    try {
      const packed = await pack(document);
      await store('readwrite', (saves) => saves.put(packed, SAVE_KEY));
      warning = null;
    } catch (e) {
      warning = `the world is not being saved: ${e?.message ?? e}`;
      sendStatus();
    }
  });
  return saving;
}

// A turn of the event loop without a timer's minimum delay.
const breathe = (() => {
  const channel = new MessageChannel();
  let wake = null;
  channel.port1.onmessage = () => wake?.();
  return () =>
    new Promise((resolve) => {
      wake = resolve;
      channel.port2.postMessage(0);
    });
})();

async function pump() {
  if (pumping || phase !== 'running') return;
  pumping = true;
  try {
    while (phase === 'running') {
      const before = last.world.tick;
      const started = performance.now();
      const { reply, body } = call({ op: 'advance', max_ticks: chunk });
      const took = performance.now() - started;
      if (reply.error) throw new Error(reply.error);
      sendFrames(body);
      const ran = reply.world.tick - before;
      if (reply.world.save_due) await save();
      if (!reply.world.catch_up) {
        if (ran > 0) live = { ticks: live.ticks + ran, ms: live.ms + took, worst: Math.max(live.worst, took / ran) };
        if (rate !== 0 || performance.now() - statusAt > HEARTBEAT_MS) {
          rate = 0;
          sendStatus();
        }
        break;
      }
      if (took > 0 && ran > 0) {
        const now = (ran * 1000) / took;
        rate = rate === 0 ? now : rate * 0.8 + now * 0.2;
        chunk = Math.max(1, Math.min(MAX_CHUNK, Math.round((chunk * SLICE_MS) / took)));
      }
      if (performance.now() - statusAt > STATUS_MS) sendStatus();
      await breathe();
    }
  } catch (e) {
    fail(e);
  } finally {
    pumping = false;
  }
}

function entropy() {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

async function handle(message) {
  switch (message.type) {
    case 'new_game': {
      const { name, pace, bots } = JSON.parse(message.text);
      const { reply } = call({ op: 'new_game', name, pace, bots, entropy: entropy() });
      error = reply.error ?? null;
      if (!reply.error) {
        phase = 'running';
        await save();
      }
      break;
    }
    case 'continue':
      if (phase === 'held') phase = 'running';
      break;
    case 'take_command':
      sendFrames(call({ op: 'take_command' }).body);
      await save();
      break;
    case 'abandon':
      await saving;
      call({ op: 'abandon' });
      if (db) await store('readwrite', (saves) => saves.delete(SAVE_KEY));
      phase = 'idle';
      error = null;
      break;
    case 'open':
    case 'close':
      call({ op: message.type });
      return;
    case 'frame': {
      const { reply, body } = call({ op: 'frame' }, encoder.encode(message.text));
      if (reply.error) console.warn(`iac-solo: ${reply.error}`);
      sendFrames(body);
      return;
    }
    case 'save':
      await save();
      return;
    default:
      console.warn(`iac-solo: unknown message ${message.type}`);
      return;
  }
  pump();
  sendStatus();
}

// The world's lock, held for the life of this worker. A reloading page's old
// worker lets go a moment after the new one starts, so it waits briefly.
function holdLock() {
  if (!navigator.locks) return Promise.resolve(true);
  return new Promise((resolve) => {
    const giveUp = new AbortController();
    const timer = setTimeout(() => giveUp.abort(), LOCK_WAIT_MS);
    navigator.locks
      .request(LOCK, { signal: giveUp.signal }, () => {
        clearTimeout(timer);
        resolve(true);
        return new Promise(() => {});
      })
      .catch(() => resolve(false));
  });
}

async function boot() {
  if (!(await holdLock())) {
    phase = 'locked';
    sendStatus();
    return;
  }
  const imports = {
    env: {
      iac_panic: (at, length) => {
        panic = decoder.decode(new Uint8Array(wasm.memory.buffer, at, length));
      },
    },
  };
  const response = await fetch(new URL('iac_solo.wasm', self.location.href));
  if (!response.ok) throw new Error(`iac_solo.wasm: HTTP ${response.status}`);
  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), imports);
  wasm = instance.exports;
  options = call({ op: 'describe' }).reply.options;

  let saved = null;
  try {
    db = await openDb();
    saved = await store('readonly', (saves) => saves.get(SAVE_KEY));
  } catch (e) {
    warning = `this browser refused storage, so the world will not be saved: ${e?.message ?? e}`;
  }
  phase = 'idle';
  if (saved) {
    const { reply } = call({ op: 'load' }, await unpack(saved));
    if (reply.world) phase = 'held';
  }
  sendStatus();
  setInterval(pump, PUMP_MS);
}

let queue = boot().catch(fail);
self.onmessage = (event) => {
  queue = queue
    .then(() => (phase === 'failed' || phase === 'locked' || phase === 'loading' ? undefined : handle(event.data)))
    .catch(fail);
};
