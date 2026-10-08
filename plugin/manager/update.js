'use strict';
// Upgrading Glass from the manager. The latest release is read from
// GitHub, its plugin zip downloaded and checked against the digest the
// release carries, the settings backed up and the installed plugin kept
// as a zip, then the player's own plugin manager replaces the plugin and
// the backend restarts so the new code loads. The kept zip goes back the
// same way.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const { get, persisting, CatalogError } = require('./catalog');
const { zipDirectory } = require('./zipwrite');
const { Zip } = require('./zip');
const signing = require('./signing');

const RELEASES_URL = 'https://api.github.com/repos/foonerd/glass/releases/latest';
const STAGING_DIR = '/tmp/plugins';
const CHECK_TTL_MS = 24 * 60 * 60 * 1000;
const MAX_RELEASE_BYTES = 1024 * 1024;
const MAX_ZIP_BYTES = 256 * 1024 * 1024;
const DOWNLOAD_TIMEOUT_MS = 15 * 60 * 1000;
const ASSET = /^glass-(\d+\.\d+\.\d+)\.zip$/;
const CONFIG_FILES = ['meter.txt', 'spectrum.txt'];
// The automatic settings backups kept: the newest five.
const KEEP_AUTOMATIC_BACKUPS = 5;

class UpdateError extends Error {
  constructor(code, message) {
    super(message || code);
    this.code = code;
  }
}

// Numeric comparison of dotted versions; a suffix after a hyphen ranks below the plain version.
function compareVersions(a, b) {
  const split = function (v) {
    const m = /^v?(\d+)\.(\d+)\.(\d+)(?:-(.*))?$/.exec(String(v || '').trim());
    return m ? [parseInt(m[1], 10), parseInt(m[2], 10), parseInt(m[3], 10), m[4] || null] : null;
  };
  const pa = split(a);
  const pb = split(b);
  if (!pa || !pb) return 0;
  for (let i = 0; i < 3; i++) {
    if (pa[i] !== pb[i]) return pa[i] < pb[i] ? -1 : 1;
  }
  if (pa[3] === pb[3]) return 0;
  if (pa[3] === null) return 1;
  if (pb[3] === null) return -1;
  return pa[3] < pb[3] ? -1 : 1;
}

// The release's zip, the plugin's unless another name is asked for, and
// what the page shows about the release.
function parseRelease(body, pattern) {
  if (!body || typeof body !== 'object' || !Array.isArray(body.assets)) {
    throw new UpdateError('no-release', 'the release has no assets');
  }
  let asset = null;
  let version = null;
  for (const a of body.assets) {
    const m = (pattern || ASSET).exec(String(a.name || ''));
    if (m) { asset = a; version = m[1]; break; }
  }
  if (!asset) throw new UpdateError('no-release', 'the release holds no zip of the name looked for');
  const digest = /^sha256:([0-9a-f]{64})$/.exec(String(asset.digest || ''));
  return {
    version: version,
    tag: String(body.tag_name || ''),
    name: String(body.name || ''),
    notes: String(body.body || '').slice(0, 20000),
    publishedAt: body.published_at || null,
    page: String(body.html_url || ''),
    prerelease: !!body.prerelease,
    url: String(asset.browser_download_url || ''),
    asset: String(asset.name || ''),
    bytes: Number(asset.size) || 0,
    sha256: digest ? digest[1] : null,
    // The sums and their signature, where the release is signed; null before signing.
    signed: signing.signedAssets(body)
  };
}

// A file held to the release's signature, where the release carries one:
// the sums and the signature fetched, the signature checked against the
// project's key, and the file's digest held to the sums' line for its
// asset. A release without a signature is as it was before signing:
// `unsigned`, installed on its digest alone. A signature that does not
// verify, or sums that do not name the asset or name another digest, is a
// release that is not as published: refused.
async function verifySigned(release, digest, fetch, job) {
  if (!release || !release.signed) return 'unsigned';
  if (job) job.state = 'verifying';
  const small = async function (what, max) {
    const res = await fetch(what.url, { limit: Math.min(max, what.bytes + 1) });
    return res.body;
  };
  let sums;
  let sig;
  try {
    sums = await small(release.signed.sums, signing.MAX_SUMS_BYTES);
    sig = await small(release.signed.sig, signing.MAX_SIG_BYTES);
  } catch (e) {
    throw new UpdateError('signature', 'the release\'s signature could not be fetched (' + (e && e.message ? e.message : e) + '); nothing was installed');
  }
  if (!signing.verifySums(sums, sig)) throw new UpdateError('signature', 'the release\'s signature does not verify; the release is not as published');
  const stated = signing.digestInSums(sums, release.asset);
  if (!stated) throw new UpdateError('signature', 'the release\'s signed sums do not name ' + release.asset);
  if (stated !== digest) throw new UpdateError('signature', 'the file does not match the signed digest of ' + release.asset + '; it is not the release as published');
  return 'signed';
}

// The newest of a list of releases that carries the zip, by version:
// pre-releases among them, drafts and releases without the zip left out.
function newestRelease(list, pattern) {
  if (!Array.isArray(list)) throw new UpdateError('bad-release', 'the releases answer is not a list');
  let best = null;
  for (const body of list) {
    if (!body || body.draft) continue;
    let release;
    try { release = parseRelease(body, pattern); } catch (e) { continue; }
    if (!best || compareVersions(release.version, best.version) > 0) best = release;
  }
  if (!best) throw new UpdateError('no-release', 'no release holds a zip of the name looked for');
  return best;
}

// The release a player is offered: the repository's latest, which is never
// a pre-release; or, on a player set to take test releases, the newest of
// its last ten whatever its mark. `latestUrl` is the address of the latest.
async function offered(fetch, latestUrl, test, pattern) {
  const res = await fetch(test ? latestUrl.replace(/\/latest$/, '?per_page=10') : latestUrl, {
    headers: { accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
    limit: MAX_RELEASE_BYTES
  });
  let body;
  try {
    body = JSON.parse(res.body.toString('utf8'));
  } catch (e) {
    throw new UpdateError('bad-release', 'the release answer is not JSON');
  }
  return test ? newestRelease(body, pattern) : parseRelease(body, pattern);
}

// The release of one version, looked up by its tag, for a file brought by
// hand: the release states the digest the file is held to.
async function released(fetch, latestUrl, version, pattern) {
  const res = await fetch(latestUrl.replace(/\/latest$/, '/tags/v' + version), {
    headers: { accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
    limit: MAX_RELEASE_BYTES
  });
  let body;
  try {
    body = JSON.parse(res.body.toString('utf8'));
  } catch (e) {
    throw new UpdateError('bad-release', 'the release answer is not JSON');
  }
  return parseRelease(body, pattern);
}

// What a zip brought by hand is: the plugin's package at its root, named
// glass, with a version, and the least glass-evo it works with.
async function examineGlassZip(file) {
  const zip = await Zip.open(file);
  try {
    const entry = zip.entries.find(function (e) { return e.isRegular && e.name === 'package.json'; });
    if (!entry && zip.entries.some(function (e) { return e.isRegular && e.name === 'manifest.json'; })) throw new UpdateError('bad-zip', 'the zip is a glass-evo release; choose it as the glass-evo zip');
    if (!entry || entry.size > MAX_RELEASE_BYTES) throw new UpdateError('bad-zip', 'the zip holds no package.json at its root; it is not a Glass release');
    let pkg;
    try {
      pkg = JSON.parse((await zip.read(entry)).toString('utf8'));
    } catch (e) {
      throw new UpdateError('bad-zip', 'the package.json in the zip is not JSON');
    }
    if (!pkg || pkg.name !== 'glass' || !/^\d+\.\d+\.\d+$/.test(String(pkg.version || ''))) throw new UpdateError('bad-zip', 'the zip is not a Glass release');
    const least = pkg.glassEvo && /^\d+\.\d+\.\d+$/.test(String(pkg.glassEvo.least || '')) ? String(pkg.glassEvo.least) : null;
    return { version: String(pkg.version), least: least };
  } finally {
    await zip.close();
  }
}

// An archive file held to the signature it carries inside, through the
// Manager's own zip reader: the result of `signing.verifyArchive`, null
// for an archive without one.
async function verifyArchiveFile(file, pem) {
  const zip = await Zip.open(file);
  try {
    return await signing.verifyArchive(zip, pem);
  } finally {
    await zip.close();
  }
}

// A file's SHA-256, streamed.
function digestOf(file) {
  return new Promise(function (resolve, reject) {
    const hash = require('crypto').createHash('sha256');
    fs.createReadStream(file).on('data', function (c) { hash.update(c); }).on('error', reject).on('end', function () { resolve(hash.digest('hex')); });
  });
}

// A file brought by hand held to the release of its version: the release
// looked up by its tag, the file's size and digest as the release states
// them. A file that cannot be verified is not installed: the release not
// looked up (no way to GitHub), stating no digest, or not matching.
async function verifyAgainstRelease(fetch, latestUrl, version, file, pattern, job) {
  if (job) job.state = 'verifying';
  let release;
  try {
    release = await released(fetch, latestUrl, version, pattern);
  } catch (e) {
    throw new UpdateError('unverified', 'release ' + version + ' could not be looked up to verify the file (' + (e && e.message ? e.message : e) + '); nothing was installed');
  }
  if (!release.sha256) throw new UpdateError('unverified', 'release ' + version + ' states no digest for its zip; nothing was installed');
  const stat = await fsp.stat(file);
  if (release.bytes && stat.size !== release.bytes) throw new UpdateError('size', 'the file is ' + stat.size + ' bytes, the release zip ' + release.bytes + '; it is not the release as published');
  const digest = await digestOf(file);
  if (digest !== release.sha256) throw new UpdateError('checksum', 'the file does not match the digest of release ' + version + '; it is not the release as published');
  release.signature = await verifySigned(release, digest, fetch, job);
  return release;
}

// A release's zip fetched to a file and checked: its size and its digest
// as the release states them. The file is there only when both hold. A
// connection that breaks has the download made again from its first byte.
// options: `fetch` in place of the manager's own, `logger`, and for tests
// `waits` and `sleep`.
async function fetchChecked(release, file, job, options) {
  options = options || {};
  const tmp = file + '.part';
  const crypto = require('crypto');
  const once = async function () {
    const hash = crypto.createHash('sha256');
    const out = fs.createWriteStream(tmp);
    let received = 0;
    job.state = 'downloading';
    job.progress = { done: 0, total: release.bytes };
    await new Promise(function (resolve, reject) {
      out.on('error', reject);
      (options.fetch || get)(release.url, {
        timeout: DOWNLOAD_TIMEOUT_MS,
        limit: release.bytes,
        sink: function (chunk, total) {
          hash.update(chunk);
          received = total;
          out.write(chunk);
          job.progress = { done: received, total: release.bytes };
        }
      }).then(function () { out.end(resolve); }, function (e) { out.destroy(); reject(e); });
    });
    job.state = 'verifying';
    if (received !== release.bytes) throw new UpdateError('size', 'downloaded ' + received + ' bytes, expected ' + release.bytes);
    const digest = hash.digest('hex');
    if (digest !== release.sha256) throw new UpdateError('checksum', 'the download does not match the release digest');
    // A signed release is held to its signature too; an unsigned one is as before.
    release.signature = await verifySigned(release, digest, options.fetch || get, job);
  };
  try {
    await persisting(once, {
      waits: options.waits,
      sleep: options.sleep,
      told: function (e, n, wait) {
        (options.logger || console).info('glass: manager: the download of ' + path.basename(file) + ' broke (' + e.message + '); made again in ' + (wait / 1000) + ' s, attempt ' + (n + 1));
      }
    });
    await fsp.rename(tmp, file);
  } catch (e) {
    await fsp.rm(tmp, { force: true });
    await fsp.rm(file, { force: true });
    throw e instanceof UpdateError || e instanceof CatalogError ? e : new UpdateError('network', e.message);
  }
  if (release.signature === 'signed') (options.logger || console).info('glass: manager: ' + path.basename(file) + ' verified against the release\'s signature');
}

class Updater {
  // dir: where the kept zip, the configuration snapshot and the state
  // live; plugin: the plugin's methods (backupCreate, updateApply,
  // restartBackend); pluginPath: the installed plugin directory; test:
  // whether this player takes test releases, asked at every check.
  constructor(options) {
    this.dir = options.dir;
    this.version = options.version;
    this.pluginPath = options.pluginPath;
    this.plugin = options.plugin;
    this.logger = options.logger || console;
    this.releasesUrl = options.releasesUrl || RELEASES_URL;
    this.fetch = options.fetch || get;
    this.test = options.test || function () { return false; };
    this.stagingDir = options.stagingDir || STAGING_DIR;
    // The project's key unless another PEM is given (tests sign with their own).
    this.publicKey = options.publicKey || null;
    this.latest = null;
    this.checkedAt = null;
    this.state = {};
  }

  async init() {
    await fsp.mkdir(path.join(this.dir, 'config'), { recursive: true });
    try {
      const saved = JSON.parse(await fsp.readFile(path.join(this.dir, 'latest.json'), 'utf8'));
      if (saved && saved.latest && saved.latest.version) {
        this.latest = saved.latest;
        this.checkedAt = saved.checkedAt || null;
      }
    } catch (e) { /* not checked yet */ }
    try {
      const state = JSON.parse(await fsp.readFile(path.join(this.dir, 'state.json'), 'utf8'));
      if (state && typeof state === 'object') this.state = state;
    } catch (e) { /* nothing kept yet */ }
    // An upgrade that was restarting when this code loaded: did it take?
    const last = this.state.last;
    if (last && last.phase === 'restarting') {
      last.phase = 'done';
      last.ok = last.to === this.version;
      last.endedAt = new Date().toISOString();
      await this.saveState();
      this.logger.info('glass: manager upgrade from ' + last.from + ' to ' + last.to + (last.ok ? ' took' : ' did not take, running ' + this.version));
    }
    if (typeof this.plugin.backupPruneAutomatic === 'function') {
      try { this.plugin.backupPruneAutomatic(KEEP_AUTOMATIC_BACKUPS); } catch (e) { /* said in the log */ }
    }
  }

  async saveState() {
    const file = path.join(this.dir, 'state.json');
    await fsp.writeFile(file + '.tmp', JSON.stringify(this.state, null, 1));
    await fsp.rename(file + '.tmp', file);
  }

  // The release offered, from GitHub when the last look is older than a day or `force`.
  async check(force) {
    const fresh = this.latest && this.checkedAt && (Date.now() - new Date(this.checkedAt).getTime()) < CHECK_TTL_MS;
    if (!force && fresh) return this.view();
    this.latest = await offered(this.fetch, this.releasesUrl, !!this.test(), ASSET);
    this.checkedAt = new Date().toISOString();
    const file = path.join(this.dir, 'latest.json');
    await fsp.writeFile(file + '.tmp', JSON.stringify({ latest: this.latest, checkedAt: this.checkedAt }));
    await fsp.rename(file + '.tmp', file);
    return this.view();
  }

  // The release last seen is forgotten, and the next look asks afresh:
  // for when what this player is offered changes under the answer kept.
  async forget() {
    this.latest = null;
    this.checkedAt = null;
    await fsp.rm(path.join(this.dir, 'latest.json'), { force: true });
  }

  previous() {
    const p = this.state.previous;
    if (!p || !p.zip || !fs.existsSync(p.zip)) return null;
    return { version: p.version, at: p.at, bytes: p.bytes || null };
  }

  view() {
    return {
      current: this.version,
      test: !!this.test(),
      latest: this.latest,
      checkedAt: this.checkedAt,
      available: !!(this.latest && compareVersions(this.latest.version, this.version) > 0),
      previous: this.previous(),
      last: this.state.last || null
    };
  }

  // Download the latest release's zip into the staging directory and
  // check it against the release's digest.
  async download(job) {
    const latest = this.latest;
    if (!latest) throw new UpdateError('no-release', 'no release known; check first');
    if (compareVersions(latest.version, this.version) <= 0) throw new UpdateError('up-to-date', 'Glass ' + this.version + ' is the latest');
    return this.stage(job, latest);
  }

  // The latest release that is not a test release, asked afresh, whatever
  // this player takes: what "back to the stable release" goes to.
  async stable() {
    return offered(this.fetch, this.releasesUrl, false, ASSET);
  }

  // A release's zip in the staging directory, checked against the
  // release's digest: the latest for an upgrade, the stable one for the
  // way back to it, newer than what is installed or not.
  async stage(job, release) {
    if (!release || !release.version) throw new UpdateError('no-release', 'no release to stage');
    if (!release.sha256) throw new UpdateError('no-digest', 'the release carries no checksum for its zip');
    if (!release.bytes || release.bytes > MAX_ZIP_BYTES) throw new UpdateError('bad-release', 'the release zip has an unusable size');
    await fsp.mkdir(this.stagingDir, { recursive: true });
    const name = 'glass-' + release.version + '.zip';
    const file = path.join(this.stagingDir, name);
    await fetchChecked(release, file, job, { fetch: this.fetch, logger: this.logger });
    return { name: name, file: file, version: release.version };
  }

  // A zip brought by hand (a player that cannot reach GitHub's files),
  // examined, verified against the release of its version, and in the
  // staging directory under the release's name. Any version but the one
  // that runs: older, for a way back to a given release, as well as newer.
  async stageFile(job, file) {
    const found = await examineGlassZip(file);
    if (found.version === this.version) throw new UpdateError('same-version', 'Glass ' + found.version + ' is what runs; nothing to install');
    // An archive that carries its own signature is verified here, with no
    // network; one without (published before signing) is held to the
    // release on GitHub.
    if (job) job.state = 'verifying';
    const inside = await verifyArchiveFile(file, this.publicKey);
    const release = inside ? { version: found.version, asset: 'glass-' + found.version + '.zip', verified: 'archive', files: inside.files }
      : Object.assign(await verifyAgainstRelease(this.fetch, this.releasesUrl, found.version, file, ASSET, job), { verified: 'release' });
    this.logger.info('glass: manager: ' + path.basename(file) + (inside ? ' verified by the signature it carries (' + inside.files + ' files)' : ' verified against release ' + found.version + ' on GitHub'));
    await fsp.mkdir(this.stagingDir, { recursive: true });
    const name = 'glass-' + found.version + '.zip';
    const to = path.join(this.stagingDir, name);
    if (path.resolve(file) !== path.resolve(to)) {
      await fsp.rm(to, { force: true });
      try {
        await fsp.rename(file, to);
      } catch (e) {
        await fsp.copyFile(file, to);
        await fsp.rm(file, { force: true });
      }
    }
    return { name: name, file: to, version: found.version, least: found.least, release: release, verified: release.verified, files: release.files || null };
  }

  // The kept zip of the version before the last upgrade, staged for the
  // plugin manager.
  async stagePrevious(job) {
    const p = this.state.previous;
    if (!p || !p.zip || !fs.existsSync(p.zip)) throw new UpdateError('no-previous', 'no previous version is kept');
    await fsp.mkdir(this.stagingDir, { recursive: true });
    const name = 'glass-' + p.version + '.zip';
    const file = path.join(this.stagingDir, name);
    job.state = 'verifying';
    await fsp.copyFile(p.zip, file);
    return { name: name, file: file, version: p.version };
  }

  // Replace the installed plugin with a staged zip: settings backed up,
  // the installed plugin kept as a zip, the configuration files
  // snapshotted and put back after the plugin manager has run its
  // install script, then the backend restarted. `options.backup` names a
  // settings backup the caller made already, and none is made here.
  async apply(job, staged, options) {
    const self = this;
    job.state = 'backing-up';
    const backup = options && options.backup ? { name: options.backup } : self.plugin.backupCreate(self.backupName(staged.version), { automatic: true });
    if (backup.error) self.logger.warn('glass: manager upgrade: settings backup ' + backup.error);
    if (typeof self.plugin.backupPruneAutomatic === 'function') {
      try { self.plugin.backupPruneAutomatic(KEEP_AUTOMATIC_BACKUPS); } catch (e) { /* said in the log */ }
    }
    for (const name of CONFIG_FILES) {
      try { await fsp.copyFile(path.join(self.pluginPath, 'config', name), path.join(self.dir, 'config', name)); } catch (e) { /* not there */ }
    }
    const kept = path.join(self.dir, 'previous-' + self.version + '.zip');
    const before = self.state.previous && self.state.previous.zip;
    const wrote = await zipDirectory(self.pluginPath, kept, {
      onProgress: function (done, total) { job.progress = { done: done, total: total }; }
    });
    if (before && before !== kept) await fsp.rm(before, { force: true });
    self.state.previous = { version: self.version, zip: kept, bytes: wrote.bytes, files: wrote.files, at: new Date().toISOString() };
    self.state.last = { from: self.version, to: staged.version, at: new Date().toISOString(), phase: 'applying', backup: backup.name || null };
    await self.saveState();

    job.state = 'applying';
    job.progress = { done: 0, total: 0 };
    try {
      await self.plugin.updateApply(staged.name);
    } catch (e) {
      self.state.last.phase = 'failed';
      self.state.last.error = String(e && e.message ? e.message : e);
      await self.saveState();
      throw new UpdateError('apply', self.state.last.error);
    }
    // The install script copied fresh templates over an empty config/;
    // the player's own files go back before the new code reads them.
    for (const name of CONFIG_FILES) {
      try { await fsp.copyFile(path.join(self.dir, 'config', name), path.join(self.pluginPath, 'config', name)); } catch (e) { /* none kept */ }
    }
    self.state.last.phase = 'restarting';
    await self.saveState();
    job.state = 'restarting';
    job.target = staged.version;
    self.plugin.restartBackend();
    return { from: self.version, to: staged.version };
  }

  // A name for an automatic settings backup: `before-<version>` for an
  // upgrade, `before-<word>` for another act, with the time where taken.
  backupName(version) {
    const base = 'before-' + version;
    const names = (this.plugin.backupList() || []).map(function (b) { return b.name; });
    if (names.indexOf(base) === -1) return base;
    const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15).replace('T', '-');
    return base + '-' + stamp;
  }
}

// Whether a settings backup is one an upgrade wrote on its own, and so one
// of those kept to the newest few: its manifest says so, either way. One
// whose manifest says nothing was made before backups said it, and is taken
// for automatic by the name the upgrades gave theirs, `before-<version>`.
// A backup a user makes says it is not, whatever it is named.
function automaticBackup(manifest, name) {
  const said = manifest && typeof manifest === 'object' ? manifest.automatic : undefined;
  if (said === true) return true;
  if (said === false) return false;
  return /^before-\d+\.\d+\.\d+(-\d{8}-\d{6})?$/.test(String(name || ''));
}

module.exports = { automaticBackup: automaticBackup, Updater: Updater, UpdateError: UpdateError, compareVersions: compareVersions, parseRelease: parseRelease, newestRelease: newestRelease, offered: offered, released: released, fetchChecked: fetchChecked, verifySigned: verifySigned, verifyArchiveFile: verifyArchiveFile, examineGlassZip: examineGlassZip, digestOf: digestOf, verifyAgainstRelease: verifyAgainstRelease, RELEASES_URL: RELEASES_URL, KEEP_AUTOMATIC_BACKUPS: KEEP_AUTOMATIC_BACKUPS, MAX_ZIP_BYTES: MAX_ZIP_BYTES };
