'use strict';
// The feed behind the Face tab: what a browser page needs live, as one
// server-sent event stream. The frames come from glass-serve's pages
// socket, one upstream connection shared by every page and opened only
// while a page is connected; the plugin's own lines (state, infinity,
// config, showing, queue) come from the plugin as it pushes them to
// displays.
//
// Events, as the page sees them:
//   event: hop      data: <the frames datagram, base64>
//   event: plugin   data: <a line of the plugin's, the JSON object with its kind>
// A page that connects gets the plugin's last config, the state as it
// stands now (the position moved on while the player plays), showing,
// infinity and queue first, so its first frame is not painted from nothing.

const http = require('http');

const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 15000;

class FaceFeed {
  constructor(opts) {
    this.socketPath = opts.socketPath;
    this.logger = opts.logger || { info() {}, warn() {} };
    // What the plugin holds now, for a page that connects before any push:
    // `{ state, infinity, showing, queue }` as the channel keeps them.
    this.current = typeof opts.current === 'function' ? opts.current : function () { return {}; };
    this.clients = new Set();
    this.upstream = null;
    this.upstreamLive = false;
    this.retry = null;
    this.backoff = RETRY_MIN_MS;
    this.last = { config: null, state: null, showing: null, infinity: null, queue: null, persist: null, views: null, weather: null };
    this.hops = 0;
  }

  // A browser page connects: the headers of an event stream, the plugin's
  // last words, then everything as it comes.
  attach(req, res) {
    const self = this;
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
      'X-Accel-Buffering': 'no'
    });
    res.write('retry: 2000\n\n');
    self.clients.add(res);
    var now = {};
    try { now = self.current() || {}; } catch (e) { /* nothing held */ }
    var opening = {
      config: self.last.config,
      state: now.state ? JSON.stringify({ kind: 'state', state: now.state }) : self.last.state,
      showing: self.last.showing || (now.showing ? JSON.stringify({ kind: 'showing', theme: now.showing.theme, meter: now.showing.meter }) : null),
      infinity: self.last.infinity || (now.infinity !== undefined && now.infinity !== null ? JSON.stringify({ kind: 'infinity', on: !!now.infinity }) : null),
      queue: self.last.queue || (Array.isArray(now.queue) ? JSON.stringify({ kind: 'queue', items: now.queue }) : null),
      persist: self.last.persist || (now.persist ? JSON.stringify(now.persist) : null),
      views: self.last.views,
      weather: self.last.weather || (now.weather ? JSON.stringify(now.weather) : null)
    };
    ['config', 'state', 'showing', 'infinity', 'queue', 'persist', 'views', 'weather'].forEach(function (kind) {
      if (opening[kind]) self.write(res, 'plugin', opening[kind]);
    });
    self.write(res, 'feed', JSON.stringify({ frames: self.upstreamLive }));
    req.on('close', function () {
      self.clients.delete(res);
      if (!self.clients.size) self.disconnect();
    });
    self.connect();
  }

  // A line of the plugin's, the object a display would get over the
  // channel; the last of each kind is kept for the next page.
  push(message) {
    if (!message || typeof message !== 'object' || !message.kind) return;
    const line = JSON.stringify(message);
    if (Object.prototype.hasOwnProperty.call(this.last, message.kind)) this.last[message.kind] = line;
    for (const res of this.clients) this.write(res, 'plugin', line);
  }

  write(res, event, data) {
    try { res.write('event: ' + event + '\ndata: ' + data + '\n\n'); } catch (e) { /* the page is going */ }
  }

  broadcast(event, data) {
    for (const res of this.clients) this.write(res, event, data);
  }

  // The upstream: glass-serve's pages socket, while a page is connected.
  connect() {
    const self = this;
    if (self.upstream || !self.clients.size) return;
    const req = http.request({ socketPath: self.socketPath, path: '/events', method: 'GET' }, function (res) {
      if (res.statusCode !== 200) {
        self.logger.warn('glass: face: the frames socket answered ' + res.statusCode);
        res.resume();
        return;
      }
      self.upstreamLive = true;
      self.backoff = RETRY_MIN_MS;
      self.broadcast('feed', JSON.stringify({ frames: true }));
      let pending = '';
      res.setEncoding('utf8');
      res.on('data', function (chunk) {
        pending += chunk;
        let at;
        while ((at = pending.indexOf('\n\n')) !== -1) {
          const block = pending.slice(0, at);
          pending = pending.slice(at + 2);
          block.split('\n').forEach(function (line) {
            if (line.indexOf('data: ') === 0) {
              self.hops++;
              self.broadcast('hop', line.slice(6));
            } else if (line.indexOf(':') === 0) {
              // A keepalive: pass it on, so a page that went away is found too.
              for (const client of self.clients) { try { client.write(': keepalive\n\n'); } catch (e) { /* going */ } }
            }
          });
        }
        if (pending.length > 65536) pending = '';
      });
      res.on('end', function () { self.lost('ended'); });
      res.on('error', function (err) { self.lost(err && err.message ? err.message : String(err)); });
    });
    req.on('error', function (err) { self.lost(err && err.code ? err.code : String(err)); });
    req.end();
    self.upstream = req;
  }

  lost(why) {
    const self = this;
    if (self.upstreamLive) self.logger.info('glass: face: frames socket ' + why);
    self.upstreamLive = false;
    self.upstream = null;
    if (!self.clients.size) return;
    self.broadcast('feed', JSON.stringify({ frames: false }));
    if (self.retry) clearTimeout(self.retry);
    self.retry = setTimeout(function () {
      self.retry = null;
      self.connect();
    }, self.backoff);
    if (self.retry.unref) self.retry.unref();
    self.backoff = Math.min(self.backoff * 2, RETRY_MAX_MS);
  }

  disconnect() {
    if (this.retry) { clearTimeout(this.retry); this.retry = null; }
    if (this.upstream) {
      try { this.upstream.destroy(); } catch (e) { /* already gone */ }
      this.upstream = null;
    }
    this.upstreamLive = false;
  }

  // Every page told to go, the upstream closed.
  stop() {
    for (const res of this.clients) { try { res.end(); } catch (e) { /* going */ } }
    this.clients.clear();
    this.disconnect();
  }

  // For the status page: pages connected and whether frames flow.
  status() {
    return { pages: this.clients.size, frames: this.upstreamLive, hops: this.hops };
  }
}

module.exports = { FaceFeed };
