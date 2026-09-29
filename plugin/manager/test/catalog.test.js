'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fsp = require('fs/promises');
const http = require('http');
const os = require('os');
const path = require('path');
const { Catalog } = require('../catalog');

// A catalog server: an index and one zip, both replaceable while it runs.
function serve() {
  const state = { index: null, zip: Buffer.alloc(0), hits: { index: 0, zip: 0 } };
  const server = http.createServer(function (req, res) {
    if (req.url === '/catalog/index.json') {
      state.hits.index += 1;
      const body = JSON.stringify(state.index);
      const etag = '"' + crypto.createHash('sha256').update(body).digest('hex').slice(0, 16) + '"';
      if (req.headers['if-none-match'] === etag) { res.writeHead(304); return res.end(); }
      res.writeHead(200, { 'content-type': 'application/json', etag: etag });
      return res.end(body);
    }
    if (req.url === '/t.zip') {
      state.hits.zip += 1;
      res.writeHead(200, { 'content-type': 'application/zip', 'content-length': state.zip.length });
      return res.end(state.zip);
    }
    res.writeHead(404);
    res.end();
  });
  return new Promise(function (resolve) {
    server.listen(0, '127.0.0.1', function () {
      const base = 'http://127.0.0.1:' + server.address().port + '/';
      resolve({ state, base, close: function () { server.close(); } });
    });
  });
}

function entryFor(zip) {
  return { name: 't', zip: 't.zip', bytes: zip.length, sha256: crypto.createHash('sha256').update(zip).digest('hex'), units: [] };
}

function indexFor(base, zip) {
  return { version: 1, base: base, templates: [entryFor(zip)] };
}

test('a zip that moved on since the index was kept is fetched again against the fresh index', async function () {
  const s = await serve();
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-catalog-'));
  const old = Buffer.from('old zip contents');
  const fresh = Buffer.from('a fresh zip, larger than the old one was');
  s.state.index = indexFor(s.base, old);
  s.state.zip = old;
  const catalog = new Catalog({ dir: dir, indexUrl: s.base + 'catalog/index.json', logger: { info() {}, warn() {} } });
  await catalog.init();
  await catalog.refresh();
  assert.equal(catalog.find('t').bytes, old.length);
  // The catalog moves on: a larger zip, the index with it.
  s.state.zip = fresh;
  s.state.index = indexFor(s.base, fresh);
  const got = await catalog.downloadCurrent('t');
  assert.equal(got.entry.bytes, fresh.length, 'the entry is the fresh one');
  assert.equal((await fsp.readFile(got.file)).toString(), fresh.toString());
  assert.equal(s.state.hits.index, 2, 'the index was fetched again once');
  assert.equal(s.state.hits.zip, 2, 'the zip was downloaded twice');
  // A smaller zip the other way round: the size check, then the same again.
  const smaller = Buffer.from('small');
  s.state.zip = smaller;
  s.state.index = indexFor(s.base, smaller);
  const again = await catalog.downloadCurrent('t');
  assert.equal(again.entry.bytes, smaller.length);
  await catalog.discard(got.file);
  await catalog.discard(again.file);
  s.close();
});

test('when the index has not moved the first error stands, and the index is stale after its age', async function () {
  const s = await serve();
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-catalog-'));
  const listed = Buffer.from('what the index says');
  s.state.index = indexFor(s.base, listed);
  s.state.zip = Buffer.from('something else of the same length is served');
  const catalog = new Catalog({ dir: dir, indexUrl: s.base + 'catalog/index.json', logger: { info() {}, warn() {} } });
  await catalog.init();
  await catalog.refresh();
  await assert.rejects(catalog.downloadCurrent('t'), function (e) { return e.code === 'too-large'; });
  assert.equal(s.state.hits.index, 2, 'the index was asked again');
  s.state.zip = Buffer.from('what the index sayS');
  await assert.rejects(catalog.downloadCurrent('t'), function (e) { return e.code === 'checksum'; });
  await assert.rejects(catalog.downloadCurrent('nobody'), function (e) { return e.code === 'unknown'; });
  assert.equal(catalog.stale(60000), false, 'just fetched');
  catalog.fetchedAt = new Date(Date.now() - 11 * 60000).toISOString();
  assert.equal(catalog.stale(10 * 60000), true, 'older than ten minutes');
  assert.equal(new Catalog({ dir: dir }).stale(1), true, 'no index at all');
  s.close();
});
