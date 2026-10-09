'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { Shelf, parseTop, archiveName, releasesOf, ceilingBounded, DEFAULT_CEILING_MB } = require('../shelf');
const { buildTarGz } = require('./tarwriter');
const { buildZip } = require('./zipwriter');

const quiet = { info: function () {}, warn: function () {}, error: function () {} };

function tmp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'glass-shelf-'));
}

function pair() {
  const k = crypto.generateKeyPairSync('ed25519');
  return { priv: k.privateKey, pub: k.publicKey.export({ type: 'spki', format: 'pem' }) };
}

// A remote's archive as the release workflow lays it out and signs it:
// the files under `<asset><version>-<arch>/`, MANIFEST and MANIFEST.sig
// at the root; a zip for Windows, a gzipped tar for the rest.
function remoteArchive(dir, key, what, options) {
  options = options || {};
  const top = what.asset + what.version + '-' + what.platform;
  const binary = what.asset === 'glass-' ? 'glass' : 'glass-evo';
  const files = options.files || (what.platform.startsWith('windows')
    ? [{ name: top + '/bin/' + binary + '.exe', data: options.binary || 'exe bytes' }, { name: top + '/SDL2.dll', data: 'dll' }]
    : [{ name: top + '/bin/' + what.platform + '/' + binary, data: options.binary || 'elf bytes' }, { name: top + '/remote/linux/install.sh', data: '#!/bin/sh' }]);
  const manifest = Buffer.from(files.map(function (f) { return crypto.createHash('sha256').update(Buffer.isBuffer(f.data) ? f.data : Buffer.from(f.data)).digest('hex') + '  ' + f.name; }).join('\n') + '\n');
  const entries = files.slice();
  if (!options.unsigned) entries.unshift({ name: 'MANIFEST', data: manifest }, { name: 'MANIFEST.sig', data: crypto.sign(null, manifest, key.priv) });
  const file = path.join(dir, options.as || ('incoming-' + Math.random().toString(36).slice(2)));
  fs.writeFileSync(file, what.platform.startsWith('windows') ? buildZip(entries) : buildTarGz(entries));
  return file;
}

test('a remote\'s archive is known by the folder it is laid out as', () => {
  assert.deepEqual(parseTop('glass-0.9.30-x64'), { asset: 'glass-', product: 'Glass', binary: 'glass', version: '0.9.30', platform: 'x64' });
  assert.deepEqual(parseTop('glass-evo-0.2.30-windows-x64'), { asset: 'glass-evo-', product: 'glass-evo', binary: 'glass-evo', version: '0.2.30', platform: 'windows-x64' });
  assert.equal(parseTop('glass-0.9.30-android'), null, 'an Android package is installed by hand');
  assert.equal(parseTop('glass-0.9.30'), null, 'the player\'s own zip has no platform');
  assert.equal(parseTop('index.js'), null);
  assert.equal(archiveName(parseTop('glass-0.9.30-armv8')), 'glass-0.9.30-armv8.tar.gz');
  assert.equal(archiveName(parseTop('glass-evo-0.2.30-windows-x64')), 'glass-evo-0.2.30-windows-x64.zip');
  assert.equal(ceilingBounded(undefined), DEFAULT_CEILING_MB);
  assert.equal(ceilingBounded(4), 16);
  assert.equal(ceilingBounded(99999), 2048);
  assert.equal(ceilingBounded('200'), 200);
});

test('the archives of a product are offered in the shape GitHub\'s releases have, newest first', () => {
  const archives = [
    { name: 'glass-0.9.29-x64.tar.gz', product: 'Glass', version: '0.9.29', platform: 'x64', bytes: 10, sha256: 'a'.repeat(64) },
    { name: 'glass-0.9.30-windows-x64.zip', product: 'Glass', version: '0.9.30', platform: 'windows-x64', bytes: 20, sha256: 'b'.repeat(64) },
    { name: 'glass-evo-0.2.30-x64.tar.gz', product: 'glass-evo', version: '0.2.30', platform: 'x64', bytes: 30, sha256: 'c'.repeat(64) }
  ];
  const list = releasesOf(archives, 'Glass', 'http://player.local:5582');
  assert.deepEqual(list.map(function (r) { return r.tag_name; }), ['v0.9.30', 'v0.9.29']);
  assert.deepEqual(list[1], { tag_name: 'v0.9.29', name: 'Glass 0.9.29', draft: false, prerelease: false, html_url: 'http://player.local:5582/manage', assets: [{ name: 'glass-0.9.29-x64.tar.gz', size: 10, browser_download_url: 'http://player.local:5582/api/shelf/files/glass-0.9.29-x64.tar.gz', digest: 'sha256:' + 'a'.repeat(64) }] });
  assert.equal(releasesOf(archives, 'glass-evo', 'http://p').length, 1);
  assert.deepEqual(releasesOf(archives, 'other', 'http://p'), []);
});

test('an archive is placed by what it says it is, the one of the same kind dropped, the ceiling held, the rest refused', async () => {
  const dir = tmp();
  const key = pair();
  let ceiling = 128;
  const shelf = new Shelf({ dir: path.join(dir, 'shelf'), ceilingMb: function () { return ceiling; }, publicKey: key.pub, logger: quiet });
  assert.deepEqual(await shelf.view(), { ceilingMb: 128, usedBytes: 0, archives: [] }, 'empty before any archive');

  const placed = await shelf.place(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.29', platform: 'x64' }, { as: 'whatever-the-user-called-it.tgz' }));
  assert.equal(placed.name, 'glass-0.9.29-x64.tar.gz', 'named by what it is, not what it came as');
  assert.equal(placed.product, 'Glass');
  assert.equal(placed.platformName, 'Linux x86_64');
  assert.equal(placed.files, 2);
  assert.match(placed.sha256, /^[0-9a-f]{64}$/);
  assert.ok(fs.existsSync(path.join(dir, 'shelf', 'glass-0.9.29-x64.tar.gz')));
  assert.ok(!fs.existsSync(path.join(dir, 'whatever-the-user-called-it.tgz')), 'the received file is consumed');

  await shelf.place(remoteArchive(dir, key, { asset: 'glass-evo-', version: '0.2.29', platform: 'windows-x64' }));
  let view = await shelf.view();
  assert.deepEqual(view.archives.map(function (a) { return a.name; }), ['glass-0.9.29-x64.tar.gz', 'glass-evo-0.2.29-windows-x64.zip']);
  assert.equal(view.usedBytes, view.archives[0].bytes + view.archives[1].bytes);

  // A newer one for the same product and platform replaces the older.
  await shelf.place(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'x64' }));
  view = await shelf.view();
  assert.deepEqual(view.archives.map(function (a) { return a.name; }).sort(), ['glass-0.9.30-x64.tar.gz', 'glass-evo-0.2.29-windows-x64.zip']);
  assert.ok(!fs.existsSync(path.join(dir, 'shelf', 'glass-0.9.29-x64.tar.gz')), 'the older archive is gone');
  // Another platform of the same product stands beside it.
  await shelf.place(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'armv8' }));
  assert.equal((await shelf.view()).archives.length, 3);

  // The releases a remote reads: one per version, with the digest the shelf computed.
  const releases = await shelf.releases('Glass', 'http://p:5582');
  assert.deepEqual(releases.map(function (r) { return r.tag_name; }), ['v0.9.30']);
  assert.equal(releases[0].assets.length, 2);
  const x64 = releases[0].assets.find(function (a) { return a.name === 'glass-0.9.30-x64.tar.gz'; });
  const served = await shelf.file('glass-0.9.30-x64.tar.gz');
  assert.equal(x64.size, served.bytes);
  assert.equal(x64.digest, 'sha256:' + crypto.createHash('sha256').update(fs.readFileSync(served.path)).digest('hex'));
  assert.equal(await shelf.file('../shelf.json'), null, 'only what the index names is served');

  // The ceiling: an archive that does not fit is refused and the shelf is as it was.
  ceiling = 16;
  // Random bytes: the archive is gzipped, and the ceiling bounds what is kept, not what unpacks.
  const big = remoteArchive(dir, key, { asset: 'glass-evo-', version: '0.2.30', platform: 'armv7' }, { binary: crypto.randomBytes(17 * 1024 * 1024) });
  await assert.rejects(shelf.place(big), function (e) { return e.code === 'shelf-full' && /does not fit/.test(e.message); });
  assert.ok(!fs.existsSync(big), 'refused and removed');
  assert.equal((await shelf.view()).archives.length, 3);
  ceiling = 128;

  // Refused: unsigned, the player's own zip, an Android package, a stray file.
  const refused = async function (file, code) {
    await assert.rejects(shelf.place(file), function (e) { return e.code === code; });
    assert.ok(!fs.existsSync(file));
  };
  await refused(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'armv7' }, { unsigned: true }), 'unsigned');
  const top = 'glass-0.9.30-x64';
  await refused(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'x64' }, { files: [{ name: 'index.js', data: '//' }, { name: 'package.json', data: '{}' }] }), 'not-remote-archive');
  await refused(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'x64' }, { files: [{ name: top + '/remote/linux/install.sh', data: '#' }] }), 'not-remote-archive');
  await refused(remoteArchive(dir, key, { asset: 'glass-', version: '0.9.30', platform: 'windows-x64' }, { files: [{ name: 'AndroidManifest.xml', data: '<m/>' }, { name: 'classes.dex', data: 'dex' }] }), 'not-remote-archive');
  const stray = path.join(dir, 'stray');
  fs.writeFileSync(stray, 'neither a zip nor a tar');
  await refused(stray, 'not-a-tar');
  // Signed with another key than the shelf's: refused as not the release.
  await refused(remoteArchive(dir, pair(), { asset: 'glass-', version: '0.9.30', platform: 'armv7' }), 'signature');

  // Removed by name; an index entry whose file is gone is dropped on the next look.
  assert.equal(await shelf.remove('glass-evo-0.2.29-windows-x64.zip'), true);
  assert.equal(await shelf.remove('glass-evo-0.2.29-windows-x64.zip'), false);
  fs.rmSync(path.join(dir, 'shelf', 'glass-0.9.30-armv8.tar.gz'));
  assert.deepEqual((await shelf.view()).archives.map(function (a) { return a.name; }), ['glass-0.9.30-x64.tar.gz']);
  fs.rmSync(dir, { recursive: true, force: true });
});
