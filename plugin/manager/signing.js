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

const MANIFEST_NAME = 'MANIFEST';
const MANIFEST_SIG_NAME = 'MANIFEST.sig';
// A manifest names every file of an archive; the plugin's zip has nearly two thousand.
const MAX_MANIFEST_BYTES = 4 * 1024 * 1024;

class SignatureError extends Error {
  constructor(message) {
    super(message);
    this.code = 'signature';
  }
}

// The lines of a manifest as `sha256sum` writes them: a map of path to digest.
function parseManifest(text) {
  const out = new Map();
  String(text).split(/\r?\n/).forEach(function (line) {
    const m = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (m) out.set(m[2], m[1].toLowerCase());
  });
  return out;
}

// An archive held to the signature it carries inside: MANIFEST, every
// file's digest, and MANIFEST.sig, signed with the project's key (or the
// PEM given, for tests). `zip` is an open zip of the Manager's own reader:
// `entries` with name, isRegular and size, and `read(entry)`. Every
// regular file in the archive must be in the manifest with its digest,
// and every file the manifest names must be in the archive. Answers
// `null` for an archive that carries no manifest and no signature, as
// every release before signing; throws a SignatureError where the
// archive carries them and they do not hold. No network is used.
async function verifyArchive(zip, pem) {
  const entry = function (name) { return zip.entries.find(function (e) { return e.isRegular && e.name === name; }); };
  const manifestEntry = entry(MANIFEST_NAME);
  const sigEntry = entry(MANIFEST_SIG_NAME);
  if (!manifestEntry && !sigEntry) return null;
  if (!manifestEntry || !sigEntry) throw new SignatureError('the archive carries half a signature; it is not the release as published');
  if (manifestEntry.size > MAX_MANIFEST_BYTES || sigEntry.size > MAX_SIG_BYTES) throw new SignatureError('the archive\'s manifest or signature has an unusable size');
  const manifest = await zip.read(manifestEntry);
  const sig = await zip.read(sigEntry);
  if (!verifySums(manifest, sig, pem)) throw new SignatureError('the archive\'s signature does not verify; the archive is not the release as published');
  const listed = parseManifest(manifest.toString('utf8'));
  if (!listed.size) throw new SignatureError('the archive\'s manifest names no file');
  const seen = new Set();
  for (const e of zip.entries) {
    if (!e.isRegular || e.name === MANIFEST_NAME || e.name === MANIFEST_SIG_NAME) continue;
    const stated = listed.get(e.name);
    if (!stated) throw new SignatureError('the archive holds ' + e.name + ', which its signed manifest does not name');
    const digest = crypto.createHash('sha256').update(await zip.read(e)).digest('hex');
    if (digest !== stated) throw new SignatureError(e.name + ' does not match the archive\'s signed manifest');
    seen.add(e.name);
  }
  for (const name of listed.keys()) {
    if (!seen.has(name)) throw new SignatureError('the archive lacks ' + name + ', which its signed manifest names');
  }
  return { files: seen.size };
}

module.exports = { PUBLIC_KEY_PEM, SUMS_NAME, SIG_NAME, MANIFEST_NAME, MANIFEST_SIG_NAME, MAX_SUMS_BYTES, MAX_SIG_BYTES, SignatureError, verifySums, digestInSums, signedAssets, parseManifest, verifyArchive };
