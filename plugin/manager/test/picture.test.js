'use strict';
// A picture for a browser view is told by its content: a server that sends
// a cover with no Content-Type (the Squeezelite plugin's proxy does) is
// passed on all the same, and what is no picture is not found.
const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('http');
const { kindOf, relay } = require('../picture');

const JPEG = Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10]), Buffer.from('JFIF'), Buffer.alloc(40, 7)]);
const PNG = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), Buffer.alloc(24, 1)]);

test('a picture is known by how it begins', () => {
  assert.equal(kindOf(JPEG), 'image/jpeg');
  assert.equal(kindOf(PNG), 'image/png');
  assert.equal(kindOf(Buffer.from('GIF89a\x01\x00', 'latin1')), 'image/gif');
  assert.equal(kindOf(Buffer.from('RIFF\x10\x00\x00\x00WEBPVP8 ', 'latin1')), 'image/webp');
  assert.equal(kindOf(Buffer.from('RIFF\x10\x00\x00\x00WAVEfmt ', 'latin1')), null, 'a sound is no picture');
  assert.equal(kindOf(Buffer.from('<!doctype html>')), null);
  assert.equal(kindOf(Buffer.alloc(0)), null);
  assert.equal(kindOf(undefined), null);
  assert.equal(kindOf(Buffer.from([0xff, 0xd8])), null, 'too short to tell');
});

// A server that answers every request with `answer(res)`, and its address.
function upstream(t, answer) {
  return new Promise(function (resolve) {
    const server = http.createServer(function (req, res) { answer(res); });
    server.listen(0, '127.0.0.1', function () {
      t.after(function () { server.close(); });
      resolve('http://127.0.0.1:' + server.address().port + '/cover');
    });
  });
}

// What a relay of `target` hands a browser: status, type and bytes.
function relayed(t, target, options) {
  return new Promise(function (resolve, reject) {
    const front = http.createServer(function (req, res) {
      res.status = function (code) { res.statusCode = code; return res; };
      res.json = function (body) { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(body)); };
      relay(target, res, options);
    });
    front.listen(0, '127.0.0.1', function () {
      t.after(function () { front.close(); });
      http.get('http://127.0.0.1:' + front.address().port + '/', function (res) {
        const chunks = [];
        res.on('data', function (c) { chunks.push(c); });
        res.on('end', function () { resolve({ status: res.statusCode, type: String(res.headers['content-type'] || ''), body: Buffer.concat(chunks) }); });
      }).on('error', reject);
    });
  });
}

test('a cover sent with no Content-Type is passed on as the picture it is', async (t) => {
  // As the Squeezelite plugin's proxy sends it: the bytes, piped, no header of its own.
  const target = await upstream(t, function (res) { res.writeHead(200); res.write(JPEG.subarray(0, 5)); setTimeout(function () { res.end(JPEG.subarray(5)); }, 20); });
  const got = await relayed(t, target);
  assert.equal(got.status, 200);
  assert.equal(got.type, 'image/jpeg');
  assert.ok(got.body.equals(JPEG), 'every byte, the first ones included');
});

test('a picture called something else is passed on under its own name', async (t) => {
  const target = await upstream(t, function (res) { res.writeHead(200, { 'Content-Type': 'application/octet-stream' }); res.end(PNG); });
  const got = await relayed(t, target);
  assert.equal(got.status, 200);
  assert.equal(got.type, 'image/png');
  assert.ok(got.body.equals(PNG));
});

test('a picture the server names is passed on as before', async (t) => {
  const target = await upstream(t, function (res) { res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Content-Length': JPEG.length }); res.end(JPEG); });
  const got = await relayed(t, target);
  assert.equal(got.status, 200);
  assert.equal(got.type, 'image/jpeg');
  assert.ok(got.body.equals(JPEG));
});

test('what is no picture is not found, labelled or not, and neither is a short answer', async (t) => {
  const page = await upstream(t, function (res) { res.writeHead(200, { 'Content-Type': 'text/html' }); res.end('<html>no cover here</html>'); });
  assert.equal((await relayed(t, page)).status, 404);
  const bare = await upstream(t, function (res) { res.writeHead(200); res.end('<html>no cover here</html>'); });
  assert.equal((await relayed(t, bare)).status, 404);
  const short = await upstream(t, function (res) { res.writeHead(200); res.end(Buffer.from([0xff, 0xd8])); });
  assert.equal((await relayed(t, short)).status, 404);
  const empty = await upstream(t, function (res) { res.writeHead(200); res.end(); });
  assert.equal((await relayed(t, empty)).status, 404);
});

test('a refusal, a picture too large and a server that is not there', async (t) => {
  const gone = await upstream(t, function (res) { res.writeHead(404, { 'Content-Type': 'image/jpeg' }); res.end(JPEG); });
  assert.equal((await relayed(t, gone)).status, 404);
  const large = await upstream(t, function (res) { res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Content-Length': JPEG.length }); res.end(JPEG); });
  assert.equal((await relayed(t, large, { limit: 10 })).status, 404);
  assert.equal((await relayed(t, 'http://127.0.0.1:9/cover')).status, 502);
});
