'use strict';

const test = require('node:test');
const assert = require('node:assert');
const http = require('http');
const path = require('path');
const childProcess = require('child_process');
const { PlayHere, ROUTE } = require('../playhere');

const STANDIN = path.join(__dirname, 'fixtures', 'lite-standin.js');
const quiet = { info() {}, warn() {}, error() {} };

// A PlayHere whose lite is the stand-in, run by node; `binary` is node
// itself so the executable check holds.
function make(options) {
  options = options || {};
  const env = Object.assign({}, process.env, options.env || {});
  const spawn = (bin, args, opts) => childProcess.spawn(process.execPath, [STANDIN].concat(args), Object.assign({}, opts, { env }));
  return new PlayHere({
    binary: process.execPath, logger: quiet, spawn,
    on: () => (options.on === undefined ? true : options.on),
    cap: () => options.cap || 0,
    idle: options.idle || 30, max: options.max || 8
  });
}

// A Manager-like http server whose upgrades go to the PlayHere.
function serve(ph) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => { res.statusCode = 404; res.end(); });
    server.on('upgrade', (req, socket, head) => ph.upgrade(req, socket, head));
    server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port }));
  });
}

// A WebSocket upgrade asked of the server; resolves with what came back:
// the status (101 or the refusal), and for 101 the socket and the head.
function ask(port, urlPath) {
  return new Promise((resolve, reject) => {
    const req = http.request({
      host: '127.0.0.1', port, path: urlPath, method: 'GET',
      headers: { Connection: 'Upgrade', Upgrade: 'websocket', 'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ==', 'Sec-WebSocket-Version': '13' }
    });
    req.on('upgrade', (res, socket, head) => resolve({ status: res.statusCode, accept: res.headers['sec-websocket-accept'], socket, head }));
    req.on('response', (res) => { res.resume(); resolve({ status: res.statusCode }); });
    req.on('error', (e) => resolve({ status: 0, error: e }));
    req.end();
  });
}

// The frames of a WebSocket stream from the server (unmasked), as they come.
function frames(buffer) {
  const out = [];
  let i = 0;
  while (i + 2 <= buffer.length) {
    const opcode = buffer[i] & 0x0f;
    let len = buffer[i + 1] & 0x7f;
    let at = i + 2;
    if (len === 126) { if (at + 2 > buffer.length) break; len = buffer.readUInt16BE(at); at += 2; }
    else if (len === 127) break;
    if (at + len > buffer.length) break;
    out.push({ opcode, payload: buffer.subarray(at, at + len) });
    i = at + len;
  }
  return out;
}

function collect(socket, head, wanted) {
  return new Promise((resolve) => {
    let got = head && head.length ? Buffer.from(head) : Buffer.alloc(0);
    const look = () => { const f = frames(got); if (f.length >= wanted) resolve(f); };
    socket.on('data', (chunk) => { got = Buffer.concat([got, chunk]); look(); });
    look();
  });
}

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

test('the lite is started on the first ask, its port read from its own word, and not started twice', async () => {
  const ph = make();
  assert.strictEqual(ph.view().running, false);
  const [a, b] = await Promise.all([ph.ensure(), ph.ensure()]);
  assert.ok(a > 0);
  assert.strictEqual(a, b);
  assert.strictEqual(ph.starts, 1, 'one spawn for two askers of the same moment');
  assert.strictEqual(ph.view().running, true);
  assert.strictEqual(await ph.ensure(), a, 'a later ask finds it running');
  assert.strictEqual(ph.starts, 1);
  ph.stop('test over');
  await wait(200);
  assert.strictEqual(ph.view().running, false);
  assert.strictEqual(ph.view().error, null, 'a lite asked to leave is no fault');
});

test('the route proxies a WebSocket to the lite: the handshake, the text frames and the PCM come through', async () => {
  const ph = make();
  const { server, port } = await serve(ph);
  try {
    const got = await ask(port, ROUTE);
    assert.strictEqual(got.status, 101);
    assert.strictEqual(got.accept, 's3pPLMBiTxaQ9kYGzzhZRbK+xOo=', 'the lite answered the key the page sent');
    const f = await collect(got.socket, got.head, 4);
    assert.strictEqual(f[0].opcode, 1);
    assert.strictEqual(JSON.parse(f[0].payload.toString()).type, 'hello');
    const segment = JSON.parse(f[1].payload.toString());
    assert.strictEqual(segment.type, 'segment');
    assert.strictEqual(segment.rate, 44100);
    assert.strictEqual(f[2].opcode, 2);
    assert.strictEqual(f[2].payload.length, 882 * 4, '20 ms of s16le stereo');
    assert.strictEqual(f[2].payload.readInt16LE(4), -999, 'the second frame of the ramp, left');
    assert.strictEqual(f[2].payload.readInt16LE(6), 999, 'and its negative on the right');
    // The Manager followed the lite's word meanwhile.
    await wait(100);
    assert.deepStrictEqual(ph.view().segment.format, 's16le');
    assert.strictEqual(ph.view().listeners, 1);
    got.socket.destroy();
    await wait(200);
    assert.strictEqual(ph.view().listeners, 0, 'the lite saw the listener go');
  } finally {
    ph.stop('test over');
    server.close();
  }
});

test('while the switch is off the route refuses with 403 and no lite is started', async () => {
  const ph = make({ on: false });
  const { server, port } = await serve(ph);
  try {
    const got = await ask(port, ROUTE);
    assert.strictEqual(got.status, 403);
    assert.strictEqual(ph.starts, 0);
  } finally { server.close(); }
});

test('an upgrade at any other path is destroyed', async () => {
  const ph = make();
  const { server, port } = await serve(ph);
  try {
    const got = await ask(port, '/api/anything');
    assert.notStrictEqual(got.status, 101);
    assert.strictEqual(ph.starts, 0);
  } finally { server.close(); }
});

test('a lite that leaves at once is a fault said in the view, and the route answers 503', async () => {
  const ph = make({ env: { LITE_STANDIN: 'fail' } });
  await assert.rejects(ph.ensure());
  assert.match(String(ph.view().error), /left before it said its port/);
  assert.strictEqual(ph.view().running, false);
  const { server, port } = await serve(ph);
  try {
    const got = await ask(port, ROUTE);
    assert.strictEqual(got.status, 503);
  } finally { server.close(); }
});

test('a lite that never says its port is given up after the start time', { timeout: 10000 }, async () => {
  const ph = make({ env: { LITE_STANDIN: 'mute' } });
  await assert.rejects(ph.ensure(), /no port within/);
  await wait(200);
  assert.strictEqual(ph.view().running, false);
});

test('the lite leaves by itself after its idle time and is started again on the next ask', { timeout: 10000 }, async () => {
  const ph = make({ idle: 1 });
  const first = await ph.ensure();
  assert.ok(first > 0);
  await wait(1600);
  assert.strictEqual(ph.view().running, false, 'gone after a second without a listener');
  assert.strictEqual(ph.view().left.reason, 'idle');
  assert.strictEqual(ph.view().error, null, 'leaving idle is no fault');
  const again = await ph.ensure();
  assert.ok(again > 0);
  assert.strictEqual(ph.starts, 2);
  ph.stop('test over');
  await wait(200);
});

test('the cap is a start argument and is said in the view', async () => {
  const ph = make({ cap: 48000 });
  await ph.ensure();
  assert.strictEqual(ph.view().capRunning, 48000);
  assert.strictEqual(ph.view().cap, 48000);
  ph.stop('test over');
  await wait(200);
});
