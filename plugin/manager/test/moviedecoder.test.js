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
  const top = 'evovpeg-0.1.0-arm/';
  const files = [
    { name: top + 'bin/evovpeg', data: Buffer.from('#!/bin/sh\necho decoder\n') },
    { name: top + 'bin/evovprobe', data: Buffer.from('#!/bin/sh\necho prober\n') },
    { name: top + 'LICENSE', data: 'MIT' },
    { name: top + 'build.json', data: '{"evovpeg":"0.1.0"}' },
    { name: top + 'licenses/NOTICE', data: 'notice' }
  ];
  if (options.noProber) files.splice(1, 1);
  const manifest = Buffer.from(files.map(function (f) { return digest(Buffer.isBuffer(f.data) ? f.data : Buffer.from(f.data)) + '  ' + f.name; }).join('\n') + '\n');
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
