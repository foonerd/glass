'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const { compareVersions, parseRelease } = require('../update');
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
