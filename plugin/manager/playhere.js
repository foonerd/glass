'use strict';
// Play here: the player's own sound to a page or a remote, served by the
// lite (outcast-lite, a component the plugin ships but does not build) on
// the loopback and proxied by the Manager at one route, /anymote/listen.
// The lite is started when the first listener asks and leaves by itself
// after its idle time without one; the Manager follows its standard output
// (one JSON line per event: listening, segment, listeners, end, exit) for
// the Status tab and Find a problem, and puts its standard error in the
// journal. The wire is outcast's contract (docs/outcast-lite.md inside the
// lite's archive), not restated here: the Manager carries the bytes both
// ways and reads nothing of them.

const fs = require('fs');
const net = require('net');
const readline = require('readline');
const childProcess = require('child_process');

// The one route; every other upgrade asked of the Manager is refused.
const ROUTE = '/anymote/listen';
// The ring's tag, the `ring` word of the plugin's snippet.
const TAG = 'glass';
// Seconds without a listener after which the lite leaves by itself.
const IDLE_SECONDS = 30;
// Listeners at once; the next is told `full` by the lite and closed.
const MAX_LISTENERS = 8;
// Seconds the lite has to say its port before the start is given up.
const START_SECONDS = 5;
// What a WebSocket key and version may carry on the way to the lite.
const KEY_SHAPE = /^[A-Za-z0-9+/=]{1,64}$/;
const VERSION_SHAPE = /^[0-9]{1,3}$/;

class PlayHere {
  // options: binary (the lite's path), logger, on() (the switch), cap()
  // (the rate cap, 0 for none), and for tests spawn(), idle, max.
  constructor(options) {
    this.binary = options.binary;
    this.logger = options.logger;
    this.isOn = options.on;
    this.capOf = options.cap;
    this.spawnFn = options.spawn || childProcess.spawn;
    this.idle = options.idle || IDLE_SECONDS;
    this.max = options.max || MAX_LISTENERS;
    this.child = null;
    this.port = null;
    this.starting = null;
    this.stopping = false;
    this.capRunning = 0;
    this.startedAt = null;
    this.segment = null;
    this.listeners = 0;
    this.error = null;
    this.left = null;
    this.starts = 0;
  }

  available() {
    try { fs.accessSync(this.binary, fs.constants.X_OK); return true; } catch (e) { return false; }
  }

  // What the Status tab, Find a problem and the pages read.
  view() {
    return {
      on: this.isOn() === true,
      cap: Number(this.capOf()) || 0,
      route: ROUTE,
      available: this.available(),
      running: !!this.child && this.port !== null,
      startedAt: this.startedAt,
      capRunning: this.capRunning,
      segment: this.segment,
      listeners: this.listeners,
      max: this.max,
      error: this.error,
      left: this.left
    };
  }

  // The lite's port, started if it is not running; one start at a time,
  // every asker of the same moment waiting on it.
  ensure() {
    const self = this;
    if (self.child && self.port !== null) return Promise.resolve(self.port);
    if (self.starting) return self.starting;
    self.starting = self.start().then(function (port) { self.starting = null; return port; }, function (e) { self.starting = null; throw e; });
    return self.starting;
  }

  start() {
    const self = this;
    const cap = Number(self.capOf()) || 0;
    const args = ['--tag', TAG, '--bind', '127.0.0.1', '--port', '0', '--idle', String(self.idle), '--max', String(self.max)];
    if (cap > 0) args.push('--cap', String(cap));
    return new Promise(function (resolve, reject) {
      let child;
      try {
        child = self.spawnFn(self.binary, args, { stdio: ['ignore', 'pipe', 'pipe'] });
      } catch (e) {
        self.error = 'the lite could not be started: ' + (e && e.message ? e.message : e);
        self.logger.warn('glass: play here: ' + self.error);
        return reject(new Error(self.error));
      }
      self.starts++;
      self.child = child;
      self.port = null;
      self.stopping = false;
      self.capRunning = cap;
      self.startedAt = new Date().toISOString();
      self.segment = null;
      self.listeners = 0;
      self.error = null;
      self.left = null;
      let settled = false;
      const timer = setTimeout(function () {
        if (settled) return;
        settled = true;
        self.error = 'the lite said no port within ' + START_SECONDS + ' s';
        self.logger.warn('glass: play here: ' + self.error);
        try { child.kill('SIGTERM'); } catch (e) { /* gone already */ }
        reject(new Error(self.error));
      }, START_SECONDS * 1000);
      readline.createInterface({ input: child.stdout }).on('line', function (line) {
        let m = null;
        try { m = JSON.parse(line); } catch (e) { return; }
        if (!m || typeof m.type !== 'string') return;
        self.event(m);
        if (m.type === 'listening' && !settled) {
          const port = parseInt(String(m.address || '').split(':').pop(), 10);
          if (!(port > 0)) return;
          settled = true;
          clearTimeout(timer);
          self.port = port;
          self.logger.info('glass: play here: the lite listens on the loopback, port ' + port + (cap > 0 ? ', capped at ' + cap + ' Hz' : '') + ', idle ' + self.idle + ' s');
          resolve(port);
        }
      });
      readline.createInterface({ input: child.stderr }).on('line', function (line) {
        if (line) self.logger.info('glass: play here: ' + line);
      });
      child.on('error', function (e) {
        self.error = 'the lite could not be started: ' + (e && e.message ? e.message : e);
        self.logger.warn('glass: play here: ' + self.error);
        if (!settled) { settled = true; clearTimeout(timer); reject(new Error(self.error)); }
        if (self.child === child) self.gone();
      });
      child.on('exit', function (code, signal) {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          self.error = 'the lite left before it said its port' + (code !== null && code !== 0 ? ' (code ' + code + ')' : '');
          self.logger.warn('glass: play here: ' + self.error);
          reject(new Error(self.error));
        } else if (code !== 0 && !self.stopping && !(self.left && self.left.reason)) {
          self.error = 'the lite left with ' + (signal ? 'signal ' + signal : 'code ' + code);
          self.logger.warn('glass: play here: ' + self.error);
        } else {
          self.logger.info('glass: play here: the lite left' + (self.left && self.left.reason ? ' (' + self.left.reason + ')' : self.stopping ? ' as asked' : ''));
        }
        if (self.child === child) self.gone();
      });
    });
  }

  // One line of the lite's standard output.
  event(m) {
    switch (m.type) {
      case 'segment':
        this.segment = {
          rate: Number(m.rate) || 0, channels: Number(m.channels) || 0, format: String(m.format || ''), bits: Number(m.bits) || 0,
          frame_bytes: Number(m.frame_bytes) || 0, one_bit: m.one_bit === true, capped: m.capped === true, chunk_ms: Number(m.chunk_ms) || 0
        };
        break;
      case 'end': this.segment = null; break;
      case 'listeners': this.listeners = Number(m.count) || 0; break;
      case 'exit': this.left = { reason: String(m.reason || ''), seconds: Number(m.seconds) || 0 }; break;
      default: break;
    }
  }

  gone() {
    this.child = null;
    this.port = null;
    this.listeners = 0;
    this.segment = null;
  }

  // The lite asked to leave now: the switch going off, or a cap changed
  // (the cap is a start argument; listeners reconnect and the next ask
  // starts it anew).
  stop(why) {
    if (!this.child) return false;
    this.stopping = true;
    this.logger.info('glass: play here: the lite asked to leave (' + why + ')');
    try { this.child.kill('SIGTERM'); } catch (e) { /* gone already */ }
    return true;
  }

  // The Manager's one door for the sound: a WebSocket upgrade at the
  // route, proxied socket to socket to the lite on the loopback (the
  // contract's own code); any other upgrade is destroyed; the route
  // refuses while the switch is off (403) or the lite cannot run (503).
  upgrade(req, socket, head) {
    const self = this;
    const url = String(req.url || '').split('?')[0];
    if (url !== ROUTE) { socket.destroy(); return; }
    const refuse = function (code, text) {
      try { socket.end('HTTP/1.1 ' + code + ' ' + text + '\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'); } catch (e) { socket.destroy(); }
    };
    socket.on('error', function () { /* the far side went; the pipe ends */ });
    if (self.isOn() !== true) return refuse(403, 'Forbidden');
    if (!self.available()) return refuse(503, 'Service Unavailable');
    const key = String(req.headers['sec-websocket-key'] || '');
    const version = String(req.headers['sec-websocket-version'] || '13');
    if (!KEY_SHAPE.test(key) || !VERSION_SHAPE.test(version)) return refuse(400, 'Bad Request');
    self.ensure().then(function (port) {
      if (socket.destroyed) return;
      const up = net.connect(port, '127.0.0.1', function () {
        up.setNoDelay(true);
        socket.setNoDelay(true);
        up.write('GET / HTTP/1.1\r\nHost: 127.0.0.1:' + port + '\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
          'Sec-WebSocket-Key: ' + key + '\r\nSec-WebSocket-Version: ' + version + '\r\n\r\n');
        if (head && head.length) up.write(head);
        up.pipe(socket);
        socket.pipe(up);
      });
      up.on('error', function () { socket.destroy(); });
      up.on('close', function () { socket.destroy(); });
      socket.on('close', function () { up.destroy(); });
    }).catch(function () { refuse(503, 'Service Unavailable'); });
  }
}

module.exports = { PlayHere, ROUTE, TAG, IDLE_SECONDS, MAX_LISTENERS, START_SECONDS };
