'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fsp = require('fs/promises');
const http = require('http');
const os = require('os');
const path = require('path');
const { Catalog, download, downloadChecked } = require('../catalog');

// A catalog server: an index and one zip, both replaceable while it runs.
function serve() {
  const state = { index: null, zip: Buffer.alloc(0), breaks: 0, hits: { index: 0, zip: 0 } };
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
      if (state.breaks > 0) {
        state.breaks -= 1;
        return res.write(state.zip.subarray(0, 1024), function () { res.destroy(); });
      }
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

test('a theme pack whose connection breaks is downloaded again', async function (t) {
  const s = await serve();
  t.after(function () { s.close(); });
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-catalog-'));
  const zip = crypto.randomBytes(100000);
  s.state.index = indexFor(s.base, zip);
  s.state.zip = zip;
  const said = [];
  const catalog = new Catalog({ dir: dir, indexUrl: s.base + 'catalog/index.json', retryWaits: [5, 5, 5], logger: { info(line) { said.push(line); }, warn() {} } });
  await catalog.init();
  await catalog.refresh();
  s.state.breaks = 2;
  const got = await catalog.downloadCurrent('t');
  assert.equal(s.state.hits.zip, 3);
  assert.ok((await fsp.readFile(got.file)).equals(zip), 'the file is the whole zip');
  assert.equal(said.filter(function (line) { return /the download of t broke/.test(line); }).length, 2);
  assert.equal(s.state.hits.index, 1, 'a broken connection is not a moved catalog: the index is not asked again');
  // Broken more often than there are waits: the error stands.
  s.state.breaks = 9; s.state.hits.zip = 0;
  await assert.rejects(catalog.downloadCurrent('t'), function (e) { return e.code === 'network'; });
  assert.equal(s.state.hits.zip, 4);
  await fsp.rm(dir, { recursive: true, force: true });
});

test('the downloads folder is cleared at the start, folders with what they hold', async function () {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-catalog-'));
  const downloads = path.join(dir, 'downloads');
  // As a player's folder came to look: a staged package's folder among the zips, in the middle of the listing.
  await fsp.mkdir(path.join(downloads, 'package-3', '1280x720_amber'), { recursive: true });
  await fsp.writeFile(path.join(downloads, 'package-3', '1280x720_amber', 'meters.txt'), '[m]\n');
  await fsp.mkdir(path.join(downloads, 'tailor-7'), { recursive: true });
  await fsp.writeFile(path.join(downloads, 'a-first.zip'), 'zip');
  await fsp.writeFile(path.join(downloads, 'z-last.zip'), 'zip');
  const catalog = new Catalog({ dir: dir, logger: { info() {}, warn() {}, error() {} } });
  await catalog.init();
  assert.deepEqual(await fsp.readdir(downloads), [], 'nothing left, whatever came before or after a folder');
  await fsp.rm(dir, { recursive: true, force: true });
});

// A server of one file for the download rule's edges: `mode` says how it
// answers the next request (and falls back to `whole` after it).
function fileServer(bytes) {
  const state = { mode: 'whole', asked: [], held: [] };
  const server = http.createServer(function (req, res) {
    const mode = state.mode;
    state.mode = 'whole';
    state.asked.push(req.headers.range || '');
    const m = /^bytes=(\d+)-$/.exec(req.headers.range || '');
    const from = m ? parseInt(m[1], 10) : 0;
    if (m && from >= bytes.length) { res.writeHead(416, { 'content-range': 'bytes */' + bytes.length }); return res.end(); }
    const body = bytes.subarray(from);
    const head = m
      ? [206, { 'content-length': body.length, 'content-range': 'bytes ' + (mode === 'other-range' ? from + 7 : from) + '-' + (bytes.length - 1) + '/' + bytes.length }]
      : [200, { 'content-length': body.length }];
    res.writeHead(head[0], head[1]);
    // Half of what was asked for, then silence on an open connection.
    if (mode === 'stall') { state.held.push(res); return res.write(body.subarray(0, Math.floor(body.length / 2))); }
    res.end(body);
  });
  return new Promise(function (resolve) {
    server.listen(0, '127.0.0.1', function () {
      resolve({ state: state, url: 'http://127.0.0.1:' + server.address().port + '/f', close: function () { state.held.forEach(function (r) { r.destroy(); }); server.close(); } });
    });
  });
}

test('a download carries on over a silent connection, drops a part that is not the file\'s, and leaves a part alone when nothing answers', async function (t) {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-download-'));
  const bytes = crypto.randomBytes(300000);
  const sha = crypto.createHash('sha256').update(bytes).digest('hex');
  const s = await fileServer(bytes);
  t.after(function () { s.close(); });
  const file = path.join(dir, 'f.bin');
  const quick = { waits: [5, 5, 5], timeout: 200 };
  const told = [];
  const options = Object.assign({ told: function (e, said) { told.push(e.code + '@' + said.at); } }, quick);

  // Half, then silence: the silence counts as a break and the rest is asked for.
  s.state.mode = 'stall';
  let got = await download(s.url, file, options);
  assert.deepEqual(s.state.asked, ['', 'bytes=150000-']);
  assert.deepEqual(told, ['timeout@150000']);
  assert.deepEqual([got.bytes, got.resumed, got.from], [300000, 1, 0]);
  assert.ok((await fsp.readFile(file + '.part')).equals(bytes));
  await fsp.rm(file + '.part');

  // A part longer than the file, the size not known beforehand: the server refuses the range, the part goes, the file comes whole.
  s.state.asked = []; told.length = 0;
  await fsp.writeFile(file + '.part', Buffer.alloc(300001));
  got = await downloadChecked(s.url, file, Object.assign({ sha256: sha }, options));
  assert.deepEqual(s.state.asked, ['bytes=300001-', '']);
  assert.deepEqual(told, ['http-416@0']);
  assert.equal(got.from, 300001);
  assert.ok((await fsp.readFile(file)).equals(bytes));
  await fsp.rm(file);

  // A part longer than the size known: it is not asked about at all.
  s.state.asked = [];
  await fsp.writeFile(file + '.part', Buffer.alloc(300001));
  await downloadChecked(s.url, file, Object.assign({ sha256: sha, bytes: 300000 }, options));
  assert.deepEqual(s.state.asked, ['']);
  await fsp.rm(file);

  // The whole file kept as a part already: nothing is asked, the digest decides.
  s.state.asked = [];
  await fsp.writeFile(file + '.part', bytes);
  got = await downloadChecked(s.url, file, Object.assign({ sha256: sha, bytes: 300000 }, options));
  assert.deepEqual(s.state.asked, []);
  assert.deepEqual([got.resumed, got.from], [0, 300000]);
  await fsp.rm(file);

  // The server answers another part than the one asked for: the part goes and the file comes whole.
  s.state.asked = []; told.length = 0;
  await fsp.writeFile(file + '.part', bytes.subarray(0, 1000));
  s.state.mode = 'other-range';
  got = await downloadChecked(s.url, file, Object.assign({ sha256: sha }, options));
  assert.deepEqual(s.state.asked, ['bytes=1000-', '']);
  assert.deepEqual(told, ['range@0']);
  assert.ok((await fsp.readFile(file)).equals(bytes));
  await fsp.rm(file);

  // Nothing answers at all: the error stands after the waits, and the part is as it was.
  await fsp.writeFile(file + '.part', bytes.subarray(0, 1000));
  const waited = [];
  await assert.rejects(download('http://127.0.0.1:1/f', file, { waits: [5, 5], sleep: async function (ms) { waited.push(ms); } }), function (e) { return e.code === 'network'; });
  assert.deepEqual(waited, [5, 5]);
  assert.equal((await fsp.stat(file + '.part')).size, 1000);

  await fsp.rm(dir, { recursive: true, force: true });
});
