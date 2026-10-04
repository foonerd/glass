'use strict';
// A picture fetched for a browser view and passed on. What it is, is told
// by how it begins, as the display tells it: a server's word for what it
// sends is not needed, and not every server gives one. The Squeezelite
// plugin's proxy on the player passes a Lyrion server's cover on with no
// Content-Type at all; a browser's own <img> shows such a picture, and so
// must a view that draws it itself.

// The media type of a picture the display draws (JPEG, PNG, GIF, WebP) by
// its first bytes, or null.
function kindOf(bytes) {
  const b = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes || []);
  if (b.length >= 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'image/jpeg';
  if (b.length >= 8 && b.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) return 'image/png';
  if (b.length >= 6 && (b.subarray(0, 6).toString('latin1') === 'GIF87a' || b.subarray(0, 6).toString('latin1') === 'GIF89a')) return 'image/gif';
  if (b.length >= 12 && b.subarray(0, 4).toString('latin1') === 'RIFF' && b.subarray(8, 12).toString('latin1') === 'WEBP') return 'image/webp';
  return null;
}

// How many bytes tell a picture from anything else.
const TELLING = 12;

// Fetch `target` and pass it on as `res` when it is a picture: by the
// server's Content-Type, or, where that says nothing or something else, by
// its first bytes. Anything else is not found; a server that cannot be
// reached is a bad gateway. `options.limit` is the most bytes a server may
// announce; `options.timeout` the wait in milliseconds.
function relay(target, res, options) {
  const opts = options || {};
  const limit = opts.limit || 32 * 1024 * 1024;
  const lib = String(target).startsWith('https') ? require('https') : require('http');
  const notFound = function () { if (!res.headersSent) res.status(404).json({ error: 'not-found' }); };
  const request = lib.get(target, { timeout: opts.timeout || 8000 }, function (upstream) {
    const labelled = String(upstream.headers['content-type'] || '');
    const length = parseInt(upstream.headers['content-length'], 10) || 0;
    if (upstream.statusCode !== 200 || length > limit) {
      upstream.resume();
      return notFound();
    }
    const pass = function (kind, first) {
      res.setHeader('Content-Type', kind);
      res.setHeader('Cache-Control', 'no-cache');
      if (first && first.length) res.write(first);
      upstream.pipe(res);
    };
    if (labelled.toLowerCase().indexOf('image') !== -1) return pass(labelled, null);
    // No word, or another: the first bytes say.
    let head = Buffer.alloc(0);
    const read = function (chunk) {
      head = Buffer.concat([head, chunk]);
      if (head.length < TELLING) return;
      settle();
    };
    const settle = function () {
      upstream.removeListener('data', read);
      upstream.removeListener('end', settle);
      const kind = kindOf(head);
      if (!kind) {
        upstream.resume();
        return notFound();
      }
      if (upstream.readableEnded) {
        res.setHeader('Content-Type', kind);
        res.setHeader('Cache-Control', 'no-cache');
        return res.end(head);
      }
      pass(kind, head);
    };
    upstream.on('data', read);
    upstream.on('end', settle);
  });
  request.on('timeout', function () { request.destroy(new Error('timeout')); });
  request.on('error', function () { if (!res.headersSent) res.status(502).json({ error: 'unreachable' }); });
  return request;
}

// How many of the addresses a player reported for its cover are remembered.
const ADDRESSES_KEPT = 8;

// The addresses a player reported for its cover, newest last, with one more:
// each once, at most `most`. A player may give a cover a new address with
// every state it pushes (the Squeezelite plugin stamps it with the time),
// and a page asks for the one it was told a moment ago: the route that
// fetches only what the player itself reported must still know that one.
function reported(list, address, most) {
  const value = String(address || '');
  const kept = (Array.isArray(list) ? list : []).filter(function (a) { return a !== value; });
  if (value) kept.push(value);
  return kept.slice(-Math.max(1, most || ADDRESSES_KEPT));
}

module.exports = { kindOf: kindOf, relay: relay, reported: reported, ADDRESSES_KEPT: ADDRESSES_KEPT };
