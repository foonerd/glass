'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { TarGz, isZipFile } = require('../tarball');
const { verifyArchive } = require('../signing');
const { buildTarGz } = require('./tarwriter');
const { buildZip } = require('./zipwriter');

function tmp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'glass-tarball-'));
}

test('a gzipped tar is listed as a zip is, directories and long names included, and read by entry', async () => {
  const dir = tmp();
  const long = 'glass-0.9.30-x64/' + 'a-folder-with-a-long-name/'.repeat(4) + 'the-file-at-the-end-of-it.txt';
  assert.ok(long.length > 100, 'a name GNU tar writes with a long-name entry');
  const file = path.join(dir, 'a.tar.gz');
  fs.writeFileSync(file, buildTarGz([
    { name: 'glass-0.9.30-x64/', dir: true },
    { name: 'glass-0.9.30-x64/bin/x64/glass', data: 'binary bytes' },
    { name: long, data: 'far' },
    { name: 'glass-0.9.30-x64/empty', data: '' }
  ]));
  const tar = await TarGz.open(file);
  try {
    assert.deepEqual(tar.entries.map(function (e) { return [e.name, e.size, e.isRegular]; }), [
      ['glass-0.9.30-x64/', 0, false],
      ['glass-0.9.30-x64/bin/x64/glass', 12, true],
      [long, 3, true],
      ['glass-0.9.30-x64/empty', 0, true]
    ]);
    assert.equal((await tar.read(tar.entries[1])).toString(), 'binary bytes');
    assert.equal((await tar.read(tar.entries[2])).toString(), 'far');
    assert.equal((await tar.read(tar.entries[3])).length, 0);
    assert.ok(fs.existsSync(file + '.tar'), 'unpacked beside the archive while open');
  } finally {
    await tar.close();
  }
  assert.ok(!fs.existsSync(file + '.tar'), 'the unpacked tar is removed on close');
  assert.equal(await isZipFile(file), false);
  fs.writeFileSync(path.join(dir, 'z.zip'), buildZip([{ name: 'x', data: 'y' }]));
  assert.equal(await isZipFile(path.join(dir, 'z.zip')), true);
  fs.rmSync(dir, { recursive: true, force: true });
});

test('what is not a gzipped tar, or ends early, is refused and leaves nothing behind', async () => {
  const dir = tmp();
  const plain = path.join(dir, 'plain.tar.gz');
  fs.writeFileSync(plain, 'not an archive at all');
  await assert.rejects(TarGz.open(plain), function (e) { return e.code === 'not-a-tar'; });
  assert.ok(!fs.existsSync(plain + '.tar'));
  const zipped = path.join(dir, 'zipped.tar.gz');
  fs.writeFileSync(zipped, require('zlib').gzipSync(Buffer.from('a gzip of something that is not a tar at all, long enough for a header')));
  await assert.rejects(TarGz.open(zipped), function (e) { return e.code === 'not-a-tar' || e.code === 'truncated'; });
  const cut = path.join(dir, 'cut.tar.gz');
  fs.writeFileSync(cut, buildTarGz([{ name: 'top/file', data: 'x'.repeat(2000) }], { truncated: true }));
  await assert.rejects(TarGz.open(cut), function (e) { return e.code === 'truncated'; });
  assert.ok(!fs.existsSync(cut + '.tar'));
  fs.rmSync(dir, { recursive: true, force: true });
});

test('the signature inside a gzipped tar is held as a zip\'s is', async () => {
  const dir = tmp();
  const key = crypto.generateKeyPairSync('ed25519');
  const pem = key.publicKey.export({ type: 'spki', format: 'pem' });
  const files = [{ name: 'glass-0.9.30-x64/bin/x64/glass', data: 'binary' }, { name: 'glass-0.9.30-x64/remote/linux/install.sh', data: '#!/bin/sh' }];
  const manifest = Buffer.from(files.map(function (f) { return crypto.createHash('sha256').update(f.data).digest('hex') + '  ' + f.name; }).join('\n') + '\n');
  const signed = path.join(dir, 'signed.tar.gz');
  fs.writeFileSync(signed, buildTarGz([{ name: 'MANIFEST', data: manifest }, { name: 'MANIFEST.sig', data: crypto.sign(null, manifest, key.privateKey) }, { name: 'glass-0.9.30-x64/', dir: true }].concat(files)));
  let tar = await TarGz.open(signed);
  try { assert.deepEqual(await verifyArchive(tar, pem), { files: 2 }); } finally { await tar.close(); }
  const tampered = path.join(dir, 'tampered.tar.gz');
  fs.writeFileSync(tampered, buildTarGz([{ name: 'MANIFEST', data: manifest }, { name: 'MANIFEST.sig', data: crypto.sign(null, manifest, key.privateKey) }, { name: files[0].name, data: 'other' }, files[1]]));
  tar = await TarGz.open(tampered);
  try { await assert.rejects(verifyArchive(tar, pem), function (e) { return e.code === 'signature' && /does not match/.test(e.message); }); } finally { await tar.close(); }
  fs.rmSync(dir, { recursive: true, force: true });
});
