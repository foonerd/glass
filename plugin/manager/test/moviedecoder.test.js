'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { buildTarGz } = require('./tarwriter');
const { Decoder, DecoderError, parseLock, compare } = require('../moviedecoder');

const LOCK = 'NAME=evovpeg\nVERSION=0.1.0\nREPOSITORY=foonerd/evovpeg\nPROGRAMS=evovpeg evovprobe\n';

function pair() {
  const k = crypto.generateKeyPairSync('ed25519');
  return { priv: k.privateKey, pub: k.publicKey.export({ type: 'spki', format: 'pem' }) };
}

function digest(data) {
  return crypto.createHash('sha256').update(data).digest('hex');
}

// A release as the evovpeg workflow publishes one: the archive for an
// architecture with its signed MANIFEST inside, SHA256SUMS naming the
// archive, and SHA256SUMS.sig; served by a fetch of the test's own.
function release(key, options) {
  options = options || {};
  // The folder the archive unpacks to says what it is: name, version, architecture.
  const top = (options.top || 'evovpeg-0.1.0-arm') + '/';
  const files = [
    { name: top + 'bin/evovpeg', data: Buffer.from('#!/bin/sh\necho decoder\n') },
    { name: top + 'bin/evovprobe', data: Buffer.from('#!/bin/sh\necho prober\n') },
    { name: top + 'LICENSE', data: 'MIT' },
    { name: top + 'build.json', data: '{"evovpeg":"0.1.0"}' },
    { name: top + 'licenses/NOTICE', data: 'notice' }
  ];
  if (options.noProber) files.splice(1, 1);
  // A file changed after the manifest was made: the archive is not the release as published.
  const listed = files.map(function (f) { return { name: f.name, data: f.data }; });
  if (options.tampered) files[0] = { name: files[0].name, data: Buffer.from('#!/bin/sh\necho other\n') };
  const manifest = Buffer.from(listed.map(function (f) { return digest(Buffer.isBuffer(f.data) ? f.data : Buffer.from(f.data)) + '  ' + f.name; }).join('\n') + '\n');
  const entries = [{ name: 'MANIFEST', data: manifest }, { name: 'MANIFEST.sig', data: crypto.sign(null, manifest, (options.manifestKey || key).priv) }].concat(files);
  const archive = buildTarGz(entries);
  const sums = Buffer.from(digest(options.wrongSum ? Buffer.from('other') : archive) + '  evovpeg-0.1.0-arm.tar.gz\n');
  const sig = crypto.sign(null, sums, (options.sumsKey || key).priv);
  const served = {
    'https://github.com/foonerd/evovpeg/releases/download/v0.1.0/SHA256SUMS': sums,
    'https://github.com/foonerd/evovpeg/releases/download/v0.1.0/SHA256SUMS.sig': sig,
    'https://github.com/foonerd/evovpeg/releases/download/v0.1.0/evovpeg-0.1.0-arm.tar.gz': archive
  };
  const asked = [];
  const fetch = async function (url, opts) {
    asked.push(url);
    const body = served[url];
    if (!body) { const e = new Error('HTTP 404 for ' + url); e.code = 'http-404'; throw e; }
    if (opts && opts.sink) {
      const half = Math.floor(body.length / 2);
      opts.sink(body.subarray(0, half), half, body.length);
      opts.sink(body.subarray(half), body.length, body.length);
      return { status: 200, body: null, received: body.length };
    }
    return { status: 200, body: body, received: body.length };
  };
  return { fetch: fetch, asked: asked, archive: archive };
}

function fresh() {
  return path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'glass-movie-decoder-')), 'evovpeg');
}

test('the lock names the component, its version, its repository and its two programs', () => {
  const lock = parseLock('# a comment\n\n' + LOCK);
  assert.deepEqual(lock, { name: 'evovpeg', version: '0.1.0', repository: 'foonerd/evovpeg', programs: ['evovpeg', 'evovprobe'] });
  assert.throws(() => parseLock('NAME=evovpeg\nVERSION=0.1.0\n'), (e) => e instanceof DecoderError && e.code === 'bad-lock');
  assert.throws(() => parseLock(LOCK.replace('PROGRAMS=evovpeg evovprobe', 'PROGRAMS=evovpeg')), /the decoder and the prober/);
  assert.throws(() => parseLock(LOCK.replace('0.1.0', 'latest')), /no version/);
  assert.equal(compare('0.1.0', '0.1.1'), -1);
  assert.equal(compare('0.2.0', '0.1.9'), 1);
  assert.equal(compare('1.0.0', '1.0.0'), 0);
});

test('a missing component leaves the distribution programs, or none, to the display', () => {
  const dir = fresh();
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm' });
  const facts = d.facts();
  assert.equal(facts.state, 'missing');
  assert.equal(facts.installed, null);
  assert.ok(facts.source === 'distribution' || facts.source === 'none');
  if (facts.source === 'distribution') assert.match(facts.programs.decoder, /ffmpeg$/);
  else assert.equal(facts.programs, null);
  assert.equal(facts.wanted.version, '0.1.0');
});

test('the release is fetched, held to its signatures and put in place; the facts then name the component', async () => {
  const dir = fresh();
  const key = pair();
  const r = release(key);
  const lines = [];
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm', fetch: r.fetch, pem: key.pub, log: (l) => lines.push(l) });
  const job = await d.install();
  assert.equal(job.state, 'installed', JSON.stringify(job.error));
  assert.equal(job.progress.done, r.archive.length);
  assert.equal(fs.readFileSync(path.join(dir, 'bin', 'evovpeg'), 'utf8'), '#!/bin/sh\necho decoder\n');
  assert.equal(fs.statSync(path.join(dir, 'bin', 'evovprobe')).mode & 0o111, 0o111);
  assert.equal(fs.readFileSync(path.join(dir, 'VERSION'), 'utf8').trim(), '0.1.0');
  assert.equal(fs.readFileSync(path.join(dir, 'licenses', 'NOTICE'), 'utf8'), 'notice');
  assert.equal(fs.readFileSync(path.join(dir, 'LICENSE'), 'utf8'), 'MIT');
  assert.ok(!fs.existsSync(dir + '.download'), 'the download folder is gone');
  assert.ok(!fs.existsSync(dir + '.new'), 'the staging folder is gone');
  const facts = d.facts();
  assert.equal(facts.state, 'installed');
  assert.equal(facts.source, 'component');
  assert.equal(facts.installed.version, '0.1.0');
  assert.equal(facts.programs.decoder, path.join(dir, 'bin', 'evovpeg'));
  assert.equal(facts.programs.prober, path.join(dir, 'bin', 'evovprobe'));
  assert.equal(facts.job.state, 'installed');
  assert.equal(r.asked.length, 3);
  assert.match(lines.join('\n'), /evovpeg 0\.1\.0 for arm in place/);
  // A second install replaces in place: the one before is gone, nothing half-written left.
  const again = await d.install();
  assert.equal(again.state, 'installed');
  assert.ok(!fs.existsSync(dir + '.prev'));
  // Removed: the facts fall back.
  await d.remove();
  assert.equal(d.facts().state, 'missing');
  assert.ok(!fs.existsSync(dir));
});

test('a download that broke for good leaves its part, and the next fetch carries it on', async () => {
  const dir = fresh();
  const key = pair();
  const r = release(key);
  const url = 'https://github.com/foonerd/evovpeg/releases/download/v0.1.0/evovpeg-0.1.0-arm.tar.gz';
  const half = Math.floor(r.archive.length / 2);
  const ranges = [];
  let broken = true;
  // The archive's connection breaks after half while `broken`; after that a range is answered with the rest.
  const fetch = async function (target, opts) {
    if (target !== url) return r.fetch(target, opts);
    ranges.push(opts.range || 0);
    if (broken) {
      opts.onResponse(200, { 'content-length': String(r.archive.length) });
      opts.sink(r.archive.subarray(0, half), half, r.archive.length);
      const e = new (require('../catalog').CatalogError)('network', 'socket hang up');
      throw e;
    }
    const from = opts.range || 0;
    opts.onResponse(from ? 206 : 200, from ? { 'content-range': 'bytes ' + from + '-' + (r.archive.length - 1) + '/' + r.archive.length, 'content-length': String(r.archive.length - from) } : { 'content-length': String(r.archive.length) });
    opts.sink(r.archive.subarray(from), r.archive.length - from, r.archive.length - from);
    return { status: from ? 206 : 200, body: null };
  };
  const lines = [];
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm', fetch: fetch, pem: key.pub, log: (l) => lines.push(l), waits: [1, 1], sleep: async () => {} });
  let job = await d.install();
  assert.equal(job.state, 'failed');
  assert.equal(job.error.code, 'network');
  assert.deepEqual(ranges, [0, half, half], 'the first try and one for each wait, each asking for the rest');
  assert.equal(fs.statSync(path.join(dir + '.download', 'evovpeg-0.1.0-arm.tar.gz.part')).size, half, 'the part stays');
  assert.ok(!fs.existsSync(dir), 'nothing put in place');
  assert.equal(lines.filter((l) => /broke at \d+ bytes .* carried on/.test(l)).length, 2);
  // The next fetch: from where the part stood, the archive held to the signed sums, the component in place, the folder gone.
  broken = false; ranges.length = 0;
  job = await d.install();
  assert.equal(job.state, 'installed', JSON.stringify(job.error));
  assert.deepEqual(ranges, [half]);
  assert.deepEqual(job.progress, { done: r.archive.length, total: r.archive.length });
  assert.equal(d.facts().state, 'installed');
  assert.ok(!fs.existsSync(dir + '.download'), 'the download folder is gone');
  // A part of another archive in the folder goes before a fetch; the component removed takes the folder with it.
  fs.mkdirSync(dir + '.download');
  fs.writeFileSync(path.join(dir + '.download', 'evovpeg-0.0.9-arm.tar.gz.part'), 'old');
  broken = true;
  job = await d.install();
  assert.deepEqual(fs.readdirSync(dir + '.download'), ['evovpeg-0.1.0-arm.tar.gz.part']);
  await d.remove();
  assert.ok(!fs.existsSync(dir + '.download') && !fs.existsSync(dir));
});

test('the release\'s archive brought by hand goes in when it is the one file this player takes, and no other does', async () => {
  const key = pair();
  const other = pair();
  const dir = fresh();
  const files = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-decoder-files-'));
  const put = function (name, bytes) { const f = path.join(files, name); fs.writeFileSync(f, bytes); return f; };
  const lines = [];
  const none = async function () { const e = new Error('no network in this test'); e.code = 'network'; throw e; };
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm', fetch: none, pem: key.pub, log: (l) => lines.push(l) });
  // The facts name the one file, and where the release publishes it.
  assert.deepEqual(d.facts().file, {
    name: 'evovpeg-0.1.0-arm.tar.gz', arch: 'arm',
    url: 'https://github.com/foonerd/evovpeg/releases/download/v0.1.0/evovpeg-0.1.0-arm.tar.gz',
    page: 'https://github.com/foonerd/evovpeg/releases/tag/v0.1.0'
  });
  assert.equal(new Decoder({ dir: dir, lock: LOCK, arch: () => '', pem: key.pub }).facts().file, null, 'no file named while the architecture is not known');

  // Every file that is not the one: refused with why, the right file named, and nothing touched.
  const refused = async function (file, code) {
    const job = await d.installFile(file);
    assert.equal(job.state, 'failed', file);
    assert.equal(job.error.code, code, job.error.message);
    if (code !== 'signature' && code !== 'no-program') assert.match(job.error.message, /this player takes evovpeg-0\.1\.0-arm\.tar\.gz$/);
    assert.ok(!fs.existsSync(dir) && !fs.existsSync(dir + '.new'), 'nothing put in place by ' + code);
    assert.equal(d.facts().state, 'missing');
  };
  await refused(put('a.zip', Buffer.from('PK\x03\x04 not a gzipped tar')), 'not-archive');
  await refused(put('sources.tar.xz', crypto.randomBytes(4096)), 'not-archive');
  await refused(put('remote.tar.gz', release(key, { top: 'glass-0.11.49-arm' }).archive), 'not-decoder');
  await refused(put('flat.tar.gz', buildTarGz([{ name: 'evovpeg', data: Buffer.from('x') }])), 'not-decoder');
  await refused(put('other-key.tar.gz', release(key, { manifestKey: other }).archive), 'signature');
  await refused(put('tampered.tar.gz', release(key, { tampered: true }).archive), 'signature');
  await refused(put('armv8.tar.gz', release(key, { top: 'evovpeg-0.1.0-armv8' }).archive), 'other-arch');
  await refused(put('older.tar.gz', release(key, { top: 'evovpeg-0.0.9-arm' }).archive), 'other-version');
  await refused(put('newer.tar.gz', release(key, { top: 'evovpeg-0.2.0-arm' }).archive), 'other-version');
  await refused(put('no-prober.tar.gz', release(key, { noProber: true }).archive), 'no-program');
  assert.equal(lines.filter((l) => /a file refused: /.test(l)).length, 10);

  // The one file: held to the manifest it carries, put in place, a part the network had left gone with its folder.
  fs.mkdirSync(dir + '.download');
  fs.writeFileSync(path.join(dir + '.download', 'evovpeg-0.1.0-arm.tar.gz.part'), 'half');
  const right = put('evovpeg-0.1.0-arm.tar.gz', release(key).archive);
  let job = await d.installFile(right);
  assert.equal(job.state, 'installed', JSON.stringify(job.error));
  assert.equal(fs.readFileSync(path.join(dir, 'bin', 'evovpeg'), 'utf8'), '#!/bin/sh\necho decoder\n');
  assert.equal(fs.statSync(path.join(dir, 'bin', 'evovprobe')).mode & 0o111, 0o111);
  assert.equal(fs.readFileSync(path.join(dir, 'VERSION'), 'utf8').trim(), '0.1.0');
  assert.ok(fs.existsSync(path.join(dir, 'licenses', 'NOTICE')) && fs.existsSync(path.join(dir, 'build.json')));
  assert.ok(!fs.existsSync(dir + '.download') && !fs.existsSync(dir + '.new') && !fs.existsSync(dir + '.prev'));
  const facts = d.facts();
  assert.deepEqual([facts.state, facts.source, facts.installed.version], ['installed', 'component', '0.1.0']);
  assert.ok(lines.some((l) => /evovpeg 0\.1\.0 for arm in place, from a file$/.test(l)));

  // Over one that is installed: the same file again is a reinstall; a file refused leaves the installed one as it is.
  job = await d.installFile(right);
  assert.equal(job.state, 'installed');
  job = await d.installFile(put('armv8.tar.gz', release(key, { top: 'evovpeg-0.1.0-armv8' }).archive));
  assert.equal(job.error.code, 'other-arch');
  assert.equal(d.facts().state, 'installed', 'the installed decoder stands');
  assert.equal(fs.readFileSync(path.join(dir, 'bin', 'evovpeg'), 'utf8'), '#!/bin/sh\necho decoder\n');

  // An older one installed (the lock moved on): the lock's file brings it level.
  fs.writeFileSync(path.join(dir, 'VERSION'), '0.0.9\n');
  assert.equal(d.facts().state, 'older');
  job = await d.installFile(right);
  assert.deepEqual([job.state, d.facts().state], ['installed', 'installed']);

  // While a fetch runs a file is refused, and the fetch is not disturbed.
  let release_ = null;
  const held = new Decoder({ dir: fresh(), lock: LOCK, arch: () => 'arm', pem: key.pub, fetch: function () { return new Promise(function (resolve, reject) { release_ = reject; }); } });
  const running = held.install();
  await new Promise((resolve) => setImmediate(resolve));
  await assert.rejects(held.installFile(right), (e) => e instanceof DecoderError && e.code === 'busy');
  release_(Object.assign(new Error('stopped'), { code: 'http-404' }));
  assert.equal((await running).state, 'failed');
  fs.rmSync(files, { recursive: true, force: true });
});

test('a release not signed with the key, an archive the sums do not name, or one without the prober is refused and nothing is put in place', async () => {
  const key = pair();
  const other = pair();
  const refused = async function (options, code) {
    const dir = fresh();
    const r = release(key, options);
    const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm', fetch: r.fetch, pem: key.pub });
    const job = await d.install();
    assert.equal(job.state, 'failed');
    assert.equal(job.error.code, code, job.error.message);
    assert.ok(!fs.existsSync(dir), 'nothing put in place');
    assert.ok(!fs.existsSync(dir + '.new') && !fs.existsSync(dir + '.download'), 'nothing left behind');
  };
  await refused({ sumsKey: other }, 'signature');
  await refused({ wrongSum: true }, 'checksum');
  await refused({ manifestKey: other }, 'signature');
  await refused({ noProber: true }, 'no-program');
  // An architecture the release has no archive for.
  const dir = fresh();
  const r = release(key);
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'armv8', fetch: r.fetch, pem: key.pub });
  const job = await d.install();
  assert.equal(job.state, 'failed');
  assert.equal(job.error.code, 'no-asset');
  // An architecture not known at all.
  const e = new Decoder({ dir: fresh(), lock: LOCK, arch: () => '', fetch: r.fetch, pem: key.pub });
  assert.equal((await e.install()).error.code, 'arch');
});

test('a component of another version than the lock reads as older or newer', () => {
  const dir = fresh();
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'bin', 'evovpeg'), '#!/bin/sh\n', { mode: 0o755 });
  fs.writeFileSync(path.join(dir, 'bin', 'evovprobe'), '#!/bin/sh\n', { mode: 0o755 });
  fs.writeFileSync(path.join(dir, 'VERSION'), '0.0.9\n');
  const d = new Decoder({ dir: dir, lock: LOCK, arch: () => 'arm' });
  assert.equal(d.facts().state, 'older');
  assert.equal(d.facts().source, 'component');
  fs.writeFileSync(path.join(dir, 'VERSION'), '0.2.0\n');
  assert.equal(d.facts().state, 'newer');
  // A program gone from under it: not installed.
  fs.unlinkSync(path.join(dir, 'bin', 'evovprobe'));
  assert.equal(d.facts().state, 'missing');
});

test('the player fetches its decoder by itself when the picture is wanted and it has none or an older one, once an hour at most', () => {
  const { shouldFetch, RETRY_MS } = require('../moviedecoder');
  const missing = { state: 'missing', job: null };
  assert.equal(shouldFetch(missing, true, 1000, 0), true);
  assert.equal(shouldFetch(missing, false, 1000, 0), false, 'the picture not wanted');
  assert.equal(shouldFetch({ state: 'older', job: null }, true, 1000, 0), true, 'older than the lock names');
  assert.equal(shouldFetch({ state: 'installed', job: null }, true, 1000, 0), false);
  assert.equal(shouldFetch({ state: 'newer', job: null }, true, 1000, 0), false, 'a newer one stands');
  assert.equal(shouldFetch({ state: 'missing', job: { state: 'downloading' } }, true, 1000, 0), false, 'a fetch under way');
  assert.equal(shouldFetch({ state: 'missing', job: { state: 'failed' } }, true, 1000, 0), true, 'a failed job is no bar');
  assert.equal(shouldFetch(missing, true, 1000 + RETRY_MS - 1, 1000), false, 'too soon after the last try');
  assert.equal(shouldFetch(missing, true, 1000 + RETRY_MS, 1000), true, 'an hour on');
  assert.equal(shouldFetch(null, true, 1000, 0), false);
});
