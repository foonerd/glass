'use strict';
// Signed releases. A release of Glass or glass-evo carries SHA256SUMS,
// every asset's digest, and SHA256SUMS.sig, that file's Ed25519 signature
// made in the release workflow with the project's key. The public key is
// here; the player holds a release that carries a signature to it, and
// installs a release without one as it did before signing, so nothing
// published before keeps a player from upgrading. Pure: the fetching is
// the caller's.

const crypto = require('crypto');

// The project's release signing key, public half: keys/release-signing.pub
// in the repository, the same file in glass-evo's.
const PUBLIC_KEY_PEM = '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAoUKUKzZ9j8S3Kx64S7oeSfKLfb+VAWGaxA5b5ccFy58=\n-----END PUBLIC KEY-----\n';

const SUMS_NAME = 'SHA256SUMS';
const SIG_NAME = 'SHA256SUMS.sig';
// The sums are small; the signature is 64 bytes.
const MAX_SUMS_BYTES = 64 * 1024;
const MAX_SIG_BYTES = 4096;

// Whether `sig` is the signature of `sums` by the key, ours unless another
// PEM is given (tests sign with their own).
function verifySums(sums, sig, pem) {
  try {
    return crypto.verify(null, Buffer.isBuffer(sums) ? sums : Buffer.from(String(sums), 'utf8'), pem || PUBLIC_KEY_PEM, sig);
  } catch (e) {
    return false;
  }
}

// The digest the sums state for `name`, or null where they do not name it.
// A line is `<hex>  <name>` as sha256sum writes it (a `*` before the name
// for binary mode is taken too).
function digestInSums(sums, name) {
  const lines = String(Buffer.isBuffer(sums) ? sums.toString('utf8') : sums).split(/\r?\n/);
  for (const line of lines) {
    const m = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (m && m[2] === name) return m[1].toLowerCase();
  }
  return null;
}

// The two assets of a signed release in a release's body, or null where
// it carries none of them or only one; a release signed by half is not a
// signed release.
function signedAssets(body) {
  if (!body || !Array.isArray(body.assets)) return null;
  const find = function (name, max) {
    const a = body.assets.find(function (x) { return x && x.name === name; });
    if (!a || !a.browser_download_url) return null;
    const size = Number(a.size) || 0;
    if (size <= 0 || size > max) return null;
    return { url: String(a.browser_download_url), bytes: size };
  };
  const sums = find(SUMS_NAME, MAX_SUMS_BYTES);
  const sig = find(SIG_NAME, MAX_SIG_BYTES);
  return sums && sig ? { sums: sums, sig: sig } : null;
}

module.exports = { PUBLIC_KEY_PEM, SUMS_NAME, SIG_NAME, MAX_SUMS_BYTES, MAX_SIG_BYTES, verifySums, digestInSums, signedAssets };
