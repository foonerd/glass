'use strict';
// The feed behind the Face tab: frames from a socket the daemon would
// serve, the plugin's lines, both to every page as one event stream.
const test = require('node:test');
const assert = require('node:assert');
const net = require('net');
const os = require('os');
const path = require('path');
const { EventEmitter } = require('events');

const { FaceFeed } = require('../facefeed');

// A page as express hands it over: what was written, and a way to leave.
function page() {
  const req = new EventEmitter();
  const res = { head: null, out: '', writeHead(status, headers) { this.head = { status, headers }; }, write(text) { this.out += text; }, end() { this.ended = true; } };
  return { req, res, events() { return res.out.split('\n\n').filter(Boolean).map(function (block) { const lines = block.split('\n'); return { event: (lines.find(l => l.startsWith('event: ')) || '').slice(7), data: (lines.find(l => l.startsWith('data: ')) || '').slice(6), comment: lines[0].startsWith(':') }; }); } };
}

// A stand-in for glass-serve's pages socket: one chunked event stream.
function daemon(socketPath) {
  const clients = [];
  const server = net.createServer(function (conn) {
    let head = '';
    conn.on('data', function (chunk) {
      head += chunk.toString();
      if (head.indexOf('\r\n\r\n') === -1) return;
      conn.write('HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n');
      const emit = (text) => conn.write(text.length.toString(16) + '\r\n' + text + '\r\n');
      emit('retry: 1000\n\n');
      clients.push({ conn, chunk: emit });
    });
  });
  return new Promise(function (resolve) {
    server.listen(socketPath, function () {
      resolve({ send(b64) { clients.forEach(c => c.chunk('data: ' + b64 + '\n\n')); }, keepalive() { clients.forEach(c => c.chunk(': keepalive\n\n')); }, count() { return clients.length; }, close() { clients.forEach(c => c.conn.destroy()); server.close(); } });
    });
  });
}

const wait = (ms) => new Promise(r => setTimeout(r, ms));
const until = async (fn, ms = 2000) => { const end = Date.now() + ms; while (!fn()) { if (Date.now() > end) throw new Error('timed out'); await wait(10); } };

test('a page that connects gets the state as it stands now, not as it was last pushed', () => {
  const feed = new FaceFeed({ socketPath: path.join(os.tmpdir(), 'glass-face-none-' + process.pid + '.sock'), current() { return { state: { status: 'play', title: 'One', seek: 15000 } }; } });
  feed.push({ kind: 'state', state: { status: 'play', title: 'One', seek: 10000 } });
  const p = page();
  feed.attach(p.req, p.res);
  const opening = p.res.out.split('\n\n').map(function (block) { const data = block.split('\n').find(l => l.indexOf('data: ') === 0); return data ? data.slice(6) : ''; }).filter(d => d.indexOf('"kind":"state"') !== -1).map(d => JSON.parse(d))[0];
  assert.equal(opening.state.seek, 15000);
  p.req.emit('close');
});

test('a page gets the headers, the plugin\'s last words, then hops and lines as they come', async () => {
  const socketPath = path.join(os.tmpdir(), 'glass-face-test-' + process.pid + '.sock');
  const d = await daemon(socketPath);
  const feed = new FaceFeed({ socketPath });
  feed.push({ kind: 'state', state: { status: 'play', title: 'One' } });
  feed.push({ kind: 'config', version: 'v1', theme: 'T', meter: 'random' });
  const p = page();
  feed.attach(p.req, p.res);
  assert.equal(p.res.head.status, 200);
  assert.equal(p.res.head.headers['Content-Type'], 'text/event-stream');
  await until(() => feed.status().frames);
  d.send('R0xTRg==');
  await until(() => p.res.out.indexOf('R0xTRg==') !== -1);
  feed.push({ kind: 'showing', theme: 'T', meter: 'left' });
  d.keepalive();
  await until(() => p.res.out.indexOf(': keepalive') !== -1);
  const events = p.events();
  assert.equal(events[0].data, '', 'the retry hint comes first');
  assert.ok(p.res.out.startsWith('retry: 2000'));
  const plugin = events.filter(e => e.event === 'plugin').map(e => JSON.parse(e.data));
  assert.deepEqual(plugin.map(m => m.kind), ['config', 'state', 'showing']);
  assert.deepEqual(events.filter(e => e.event === 'hop').map(e => e.data), ['R0xTRg==']);
  assert.ok(events.some(e => e.event === 'feed' && JSON.parse(e.data).frames === true));
  assert.equal(feed.status().pages, 1);
  assert.equal(feed.status().hops, 1);
  p.req.emit('close');
  assert.equal(feed.status().pages, 0);
  await until(() => !feed.status().frames);
  d.close();
});

test('without the daemon the page still gets the plugin, and frames come when it appears', async () => {
  const socketPath = path.join(os.tmpdir(), 'glass-face-test-late-' + process.pid + '.sock');
  const feed = new FaceFeed({ socketPath });
  const p = page();
  feed.attach(p.req, p.res);
  feed.push({ kind: 'infinity', on: true });
  await wait(50);
  assert.ok(p.res.out.indexOf('"infinity"') !== -1);
  assert.ok(p.res.out.indexOf('"frames":false') !== -1);
  const d = await daemon(socketPath);
  await until(() => feed.status().frames, 5000);
  d.send('YWJj');
  await until(() => p.res.out.indexOf('YWJj') !== -1);
  feed.stop();
  assert.equal(feed.status().pages, 0);
  assert.ok(p.res.ended);
  d.close();
});

test('a page that connects before any push gets what the plugin holds now', async () => {
  const socketPath = path.join(os.tmpdir(), 'glass-face-test-now-' + process.pid + '.sock');
  const feed = new FaceFeed({ socketPath, current: () => ({ state: { status: 'pause', title: 'Held' }, infinity: true, showing: { theme: 'T', meter: 'm2' } }) });
  const p = page();
  feed.attach(p.req, p.res);
  const plugin = p.events().filter(e => e.event === 'plugin').map(e => JSON.parse(e.data));
  assert.deepEqual(plugin.map(m => m.kind), ['state', 'showing', 'infinity']);
  assert.equal(plugin[0].state.title, 'Held');
  assert.equal(plugin[1].meter, 'm2');
  assert.equal(plugin[2].on, true);
  feed.stop();
});

test('the queue the plugin holds or pushes reaches a page, the last of its kind', async () => {
  const socketPath = path.join(os.tmpdir(), 'glass-face-test-queue-' + process.pid + '.sock');
  const feed = new FaceFeed({ socketPath, current: () => ({ queue: [{ title: 'One', artist: '', album: '', duration: 1 }] }) });
  const p = page();
  feed.attach(p.req, p.res);
  let plugin = p.events().filter(e => e.event === 'plugin').map(e => JSON.parse(e.data));
  assert.deepEqual(plugin.map(m => m.kind), ['queue']);
  assert.equal(plugin[0].items[0].title, 'One');
  feed.push({ kind: 'queue', items: [{ title: 'Two', artist: '', album: '', duration: 2 }] });
  const later = page();
  feed.attach(later.req, later.res);
  plugin = later.events().filter(e => e.event === 'plugin').map(e => JSON.parse(e.data));
  assert.deepEqual(plugin.map(m => m.kind), ['queue']);
  assert.equal(plugin[0].items[0].title, 'Two', 'the pushed queue replaces what the plugin held');
  feed.stop();
});
