'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { PUBLIC_KEY_PEM, verifySums, digestInSums, signedAssets } = require('../signing');

// A key pair of the test's own, as the release workflow's is made.
function pair() {
  const k = crypto.generateKeyPairSync('ed25519');
  return { priv: k.privateKey, pub: k.publicKey.export({ type: 'spki', format: 'pem' }) };
}

// The repository's keys folder, looked for upward from here: the check may
// run with the plugin folder alone mounted, and then the comparison with
// the repository's file is not possible here (it runs where the repository
// is whole).
function repositoryKey() {
  let dir = __dirname;
  for (let i = 0; i < 6; i++) {
    const file = path.join(dir, 'keys', 'release-signing.pub');
    if (fs.existsSync(file)) return fs.readFileSync(file, 'utf8');
    dir = path.dirname(dir);
  }
  return null;
}

test('the public key the plugin carries is the one in the repository', (t) => {
  const file = repositoryKey();
  if (file === null) t.diagnostic('the repository\'s keys folder is not reachable from here; compared where it is');
  else assert.equal(PUBLIC_KEY_PEM.trim(), file.trim());
  assert.match(PUBLIC_KEY_PEM, /^-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA[A-Za-z0-9+/=]+\n-----END PUBLIC KEY-----\n$/, 'an Ed25519 public key as PEM');
  // A real Ed25519 public key: Node parses it and it verifies nothing signed by another key.
  const other = pair();
  const sig = crypto.sign(null, Buffer.from('x'), other.priv);
  assert.equal(verifySums('x', sig), false);
});

test('a signature over the sums verifies with the key that made it, and with no other', () => {
  const mine = pair();
  const sums = 'ab'.repeat(32) + '  glass-0.9.21.zip\n' + 'cd'.repeat(32) + '  glass-0.9.21-x64.tar.gz\n';
  const sig = crypto.sign(null, Buffer.from(sums), mine.priv);
  assert.equal(verifySums(sums, sig, mine.pub), true);
  assert.equal(verifySums(Buffer.from(sums), sig, mine.pub), true, 'as a buffer too');
  assert.equal(verifySums(sums + 'x', sig, mine.pub), false, 'a changed line');
  assert.equal(verifySums(sums, Buffer.concat([sig.subarray(0, 63), Buffer.from([sig[63] ^ 1])]), mine.pub), false, 'a changed signature');
  assert.equal(verifySums(sums, sig, pair().pub), false, 'another key');
  assert.equal(verifySums(sums, Buffer.from('short'), mine.pub), false, 'no signature at all');
});

test('the sums name each asset with its digest, as sha256sum writes them', () => {
  const sums = 'AB'.repeat(32) + '  glass-0.9.21.zip\n' + 'cd'.repeat(32) + ' *glass-0.9.21-x64.tar.gz\r\n\n';
  assert.equal(digestInSums(sums, 'glass-0.9.21.zip'), 'ab'.repeat(32));
  assert.equal(digestInSums(sums, 'glass-0.9.21-x64.tar.gz'), 'cd'.repeat(32));
  assert.equal(digestInSums(sums, 'glass-0.9.21-arm.tar.gz'), null);
  assert.equal(digestInSums(Buffer.from(sums), 'glass-0.9.21.zip'), 'ab'.repeat(32));
});

test('a release is signed when it carries both the sums and the signature, and not by half', () => {
  const asset = function (name, size) { return { name: name, size: size, browser_download_url: 'u/' + name }; };
  assert.deepEqual(signedAssets({ assets: [asset('glass-1.zip', 9), asset('SHA256SUMS', 200), asset('SHA256SUMS.sig', 64)] }), { sums: { url: 'u/SHA256SUMS', bytes: 200 }, sig: { url: 'u/SHA256SUMS.sig', bytes: 64 } });
  assert.equal(signedAssets({ assets: [asset('glass-1.zip', 9), asset('SHA256SUMS', 200)] }), null);
  assert.equal(signedAssets({ assets: [asset('glass-1.zip', 9)] }), null);
  assert.equal(signedAssets({ assets: [asset('SHA256SUMS', 200000), asset('SHA256SUMS.sig', 64)] }), null, 'sums too large to be ours');
  assert.equal(signedAssets({}), null);
});
