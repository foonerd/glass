'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const http = require('http');
const { compareVersions, parseRelease, newestRelease, offered, fetchChecked, Updater } = require('../update');
const { zipDirectory } = require('../zipwrite');
const zip = require('../zip');

test('compareVersions orders numerically and ranks a suffix below the plain version', function () {
  assert.equal(compareVersions('0.6.1', '0.6.0'), 1);
  assert.equal(compareVersions('0.6.0', '0.6.1'), -1);
  assert.equal(compareVersions('v0.10.0', '0.9.9'), 1);
  assert.equal(compareVersions('1.0.0', '1.0.0'), 0);
  assert.equal(compareVersions('1.0.0-rc1', '1.0.0'), -1);
  assert.equal(compareVersions('nonsense', '1.0.0'), 0);
});

test('parseRelease finds the plugin zip and its digest', function () {
  const release = parseRelease({
    tag_name: 'v0.6.1', name: '0.6.1', body: 'notes', published_at: '2026-09-26T14:00:00Z', html_url: 'https://example/r',
    assets: [
      { name: 'glass-0.6.1-arm.tar.gz', size: 1, digest: 'sha256:' + 'a'.repeat(64), browser_download_url: 'https://example/a' },
      { name: 'glass-0.6.1.zip', size: 1234, digest: 'sha256:' + 'b'.repeat(64), browser_download_url: 'https://example/z' }
    ]
  });
  assert.equal(release.version, '0.6.1');
  assert.equal(release.url, 'https://example/z');
  assert.equal(release.bytes, 1234);
  assert.equal(release.sha256, 'b'.repeat(64));
  assert.equal(release.notes, 'notes');
  assert.throws(function () { parseRelease({ assets: [{ name: 'other.zip' }] }); }, function (e) { return e.code === 'no-release'; });
  assert.equal(parseRelease({ assets: [{ name: 'glass-1.0.0.zip', size: 5 }] }).sha256, null);
});

test('zipDirectory writes a zip the reader opens, files at the root, links left out', async function () {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-zipw-'));
  const src = path.join(dir, 'plugin');
  await fsp.mkdir(path.join(src, 'config'), { recursive: true });
  await fsp.mkdir(path.join(src, 'node_modules', 'x'), { recursive: true });
  await fsp.writeFile(path.join(src, 'package.json'), '{"name":"glass"}');
  await fsp.writeFile(path.join(src, 'config', 'meter.txt'), '[current]\nmeter = a\n');
  const big = Buffer.alloc(300000);
  for (let i = 0; i < big.length; i++) big[i] = (i * 7) & 0xff;
  await fsp.writeFile(path.join(src, 'node_modules', 'x', 'big.bin'), big);
  await fsp.symlink('package.json', path.join(src, 'link'));
  const out = path.join(dir, 'previous.zip');
  const progress = [];
  const result = await zipDirectory(src, out, { onProgress: function (d, t) { progress.push([d, t]); } });
  assert.equal(result.files, 3);
  assert.equal(progress[progress.length - 1][0], progress[progress.length - 1][1]);
  const z = await zip.Zip.open(out);
  try {
    const names = z.entries.map(function (e) { return e.name; }).sort();
    assert.deepEqual(names, ['config/meter.txt', 'node_modules/x/big.bin', 'package.json']);
    const entry = z.entries.find(function (e) { return e.name === 'node_modules/x/big.bin'; });
    assert.equal((await z.read(entry)).equals(big), true);
    assert.equal((await z.read(z.entries.find(function (e) { return e.name === 'package.json'; }))).toString(), '{"name":"glass"}');
  } finally {
    await z.close();
  }
  await fsp.rm(dir, { recursive: true, force: true });
});

// A release as GitHub lists it, with the plugin's zip.
function listed(version, marks) {
  return Object.assign({
    tag_name: 'v' + version,
    assets: [{ name: 'glass-' + version + '.zip', size: 5, digest: 'sha256:' + 'a'.repeat(64), browser_download_url: 'zip/' + version }]
  }, marks || {});
}

test('the newest of a list is taken by version, a pre-release among them, a draft never', function () {
  const list = [listed('0.8.4'), listed('0.8.6', { draft: true }), listed('0.8.5', { prerelease: true }), listed('0.8.3'), { tag_name: 'v0.9.0', assets: [] }];
  const newest = newestRelease(list);
  assert.equal(newest.version, '0.8.5');
  assert.equal(newest.prerelease, true);
  assert.equal(parseRelease(listed('0.8.4')).prerelease, false);
  assert.throws(function () { newestRelease([listed('1.0.0', { draft: true })]); }, function (e) { return e.code === 'no-release'; });
  assert.throws(function () { newestRelease({}); }, function (e) { return e.code === 'bad-release'; });
});

test('a player is offered the latest release, or the newest of all where it takes test releases', async function () {
  const asked = [];
  const fetch = async function (url) {
    asked.push(url);
    const body = /\/latest$/.test(url) ? listed('0.8.4') : [listed('0.8.5', { prerelease: true }), listed('0.8.4')];
    return { body: Buffer.from(JSON.stringify(body)) };
  };
  assert.equal((await offered(fetch, 'https://api/repos/x/releases/latest', false)).version, '0.8.4');
  assert.equal((await offered(fetch, 'https://api/repos/x/releases/latest', true)).version, '0.8.5');
  assert.deepEqual(asked, ['https://api/repos/x/releases/latest', 'https://api/repos/x/releases?per_page=10']);
  await assert.rejects(offered(async function () { return { body: Buffer.from('<html>') }; }, 'x/latest', false), function (e) { return e.code === 'bad-release'; });
});

test('the updater asks whether test releases are taken at every check, and says so in its view', async function () {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-upd-test-'));
  let test = false;
  const updater = new Updater({
    dir: dir,
    version: '0.8.4',
    pluginPath: dir,
    plugin: {},
    logger: { info: function () {}, warn: function () {} },
    releasesUrl: 'r/latest',
    test: function () { return test; },
    fetch: async function (url) {
      return { body: Buffer.from(JSON.stringify(url === 'r/latest' ? listed('0.8.4') : [listed('0.8.5', { prerelease: true }), listed('0.8.4')])) };
    }
  });
  await updater.init();
  let view = await updater.check(true);
  assert.deepEqual([view.test, view.available, view.latest.version], [false, false, '0.8.4']);
  test = true;
  view = await updater.check(true);
  assert.deepEqual([view.test, view.available, view.latest.version, view.latest.prerelease], [true, true, '0.8.5', true]);
  await fsp.rm(dir, { recursive: true, force: true });
});

// A server of one zip that breaks the connection part way through the body
// for its first `breaks` requests, and answers `status` when one is set.
function brittle(zip) {
  const state = { hits: 0, breaks: 0, status: 0 };
  const server = http.createServer(function (req, res) {
    state.hits += 1;
    if (state.status) { res.writeHead(state.status); return res.end(); }
    res.writeHead(200, { 'content-type': 'application/zip', 'content-length': zip.length });
    if (state.hits <= state.breaks) {
      return res.write(zip.subarray(0, 4096), function () { res.destroy(); });
    }
    res.end(zip);
  });
  return new Promise(function (resolve) {
    server.listen(0, '127.0.0.1', function () {
      resolve({ state: state, url: 'http://127.0.0.1:' + server.address().port + '/glass.zip', close: function () { server.close(); } });
    });
  });
}

test('a download whose connection breaks is made again from its first byte, and only such a one', async function (t) {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-upd-retry-'));
  const zipBytes = crypto.randomBytes(200000);
  const s = await brittle(zipBytes);
  const release = { url: s.url, bytes: zipBytes.length, sha256: crypto.createHash('sha256').update(zipBytes).digest('hex') };
  const file = path.join(dir, 'glass.zip');
  const said = [];
  const waited = [];
  const options = {
    waits: [10, 20, 30],
    sleep: async function (ms) { waited.push(ms); },
    logger: { info: function (line) { said.push(line); } }
  };
  const job = { state: 'queued', progress: null };
  t.after(function () { s.close(); });

  // Twice broken, then whole: the file is the zip, nothing of the broken tries in it.
  s.state.breaks = 2;
  await fetchChecked(release, file, job, options);
  assert.equal(s.state.hits, 3);
  assert.deepEqual(waited, [10, 20]);
  assert.equal(said.length, 2);
  assert.match(said[0], /glass\.zip broke .* attempt 2$/);
  assert.ok((await fsp.readFile(file)).equals(zipBytes), 'the file is the whole zip');
  assert.deepEqual(job.progress, { done: zipBytes.length, total: zipBytes.length });
  assert.deepEqual(await fsp.readdir(dir), ['glass.zip'], 'no part file is left');

  // Broken every time: the waits run out, the error stands, no file is left.
  await fsp.rm(file);
  s.state.hits = 0; s.state.breaks = 99; waited.length = 0;
  await assert.rejects(fetchChecked(release, file, job, options), function (e) { return e.code === 'network'; });
  assert.equal(s.state.hits, 4, 'the first try and one for each wait');
  assert.deepEqual(waited, [10, 20, 30]);
  assert.deepEqual(await fsp.readdir(dir), []);

  // A zip that is not the one the release names is not asked for again.
  s.state.hits = 0; s.state.breaks = 0; waited.length = 0;
  await assert.rejects(fetchChecked(Object.assign({}, release, { sha256: 'f'.repeat(64) }), file, job, options), function (e) { return e.code === 'checksum'; });
  assert.equal(s.state.hits, 1);

  // Nor is an answer that says the zip is not there; a failing server is.
  s.state.hits = 0; s.state.status = 404;
  await assert.rejects(fetchChecked(release, file, job, options), function (e) { return e.code === 'http-404'; });
  assert.equal(s.state.hits, 1);
  s.state.hits = 0; s.state.status = 503;
  await assert.rejects(fetchChecked(release, file, job, options), function (e) { return e.code === 'http-503'; });
  assert.equal(s.state.hits, 4);
  assert.deepEqual(await fsp.readdir(dir), []);

  await fsp.rm(dir, { recursive: true, force: true });
});
