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
const { get, CatalogError } = require('./catalog');
const { zipDirectory } = require('./zipwrite');

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

// The release's plugin zip and what the page shows about the release.
function parseRelease(body) {
  if (!body || typeof body !== 'object' || !Array.isArray(body.assets)) {
    throw new UpdateError('no-release', 'the release has no assets');
  }
  let asset = null;
  let version = null;
  for (const a of body.assets) {
    const m = ASSET.exec(String(a.name || ''));
    if (m) { asset = a; version = m[1]; break; }
  }
  if (!asset) throw new UpdateError('no-release', 'the release holds no plugin zip');
  const digest = /^sha256:([0-9a-f]{64})$/.exec(String(asset.digest || ''));
  return {
    version: version,
    tag: String(body.tag_name || ''),
    name: String(body.name || ''),
    notes: String(body.body || '').slice(0, 20000),
    publishedAt: body.published_at || null,
    page: String(body.html_url || ''),
    url: String(asset.browser_download_url || ''),
    bytes: Number(asset.size) || 0,
    sha256: digest ? digest[1] : null
  };
}

class Updater {
  // dir: where the kept zip, the configuration snapshot and the state
  // live; plugin: the plugin's methods (backupCreate, updateApply,
  // restartBackend); pluginPath: the installed plugin directory.
  constructor(options) {
    this.dir = options.dir;
    this.version = options.version;
    this.pluginPath = options.pluginPath;
    this.plugin = options.plugin;
    this.logger = options.logger || console;
    this.releasesUrl = options.releasesUrl || RELEASES_URL;
    this.stagingDir = options.stagingDir || STAGING_DIR;
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

  // The latest release, from GitHub when the last look is older than a day or `force`.
  async check(force) {
    const fresh = this.latest && this.checkedAt && (Date.now() - new Date(this.checkedAt).getTime()) < CHECK_TTL_MS;
    if (!force && fresh) return this.view();
    const res = await get(this.releasesUrl, {
      headers: { accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
      limit: MAX_RELEASE_BYTES
    });
    let body;
    try {
      body = JSON.parse(res.body.toString('utf8'));
    } catch (e) {
      throw new UpdateError('bad-release', 'the release answer is not JSON');
    }
    this.latest = parseRelease(body);
    this.checkedAt = new Date().toISOString();
    const file = path.join(this.dir, 'latest.json');
    await fsp.writeFile(file + '.tmp', JSON.stringify({ latest: this.latest, checkedAt: this.checkedAt }));
    await fsp.rename(file + '.tmp', file);
    return this.view();
  }

  previous() {
    const p = this.state.previous;
    if (!p || !p.zip || !fs.existsSync(p.zip)) return null;
    return { version: p.version, at: p.at, bytes: p.bytes || null };
  }

  view() {
    return {
      current: this.version,
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
    if (!latest.sha256) throw new UpdateError('no-digest', 'the release carries no checksum for its zip');
    if (!latest.bytes || latest.bytes > MAX_ZIP_BYTES) throw new UpdateError('bad-release', 'the release zip has an unusable size');
    await fsp.mkdir(this.stagingDir, { recursive: true });
    const name = 'glass-' + latest.version + '.zip';
    const file = path.join(this.stagingDir, name);
    const tmp = file + '.part';
    const crypto = require('crypto');
    const hash = crypto.createHash('sha256');
    const out = fs.createWriteStream(tmp);
    let received = 0;
    job.state = 'downloading';
    try {
      await new Promise(function (resolve, reject) {
        out.on('error', reject);
        get(latest.url, {
          timeout: DOWNLOAD_TIMEOUT_MS,
          limit: latest.bytes,
          sink: function (chunk, total) {
            hash.update(chunk);
            received = total;
            out.write(chunk);
            job.progress = { done: received, total: latest.bytes };
          }
        }).then(function () { out.end(resolve); }, function (e) { out.destroy(); reject(e); });
      });
      job.state = 'verifying';
      if (received !== latest.bytes) throw new UpdateError('size', 'downloaded ' + received + ' bytes, expected ' + latest.bytes);
      if (hash.digest('hex') !== latest.sha256) throw new UpdateError('checksum', 'the download does not match the release digest');
      await fsp.rename(tmp, file);
      return { name: name, file: file, version: latest.version };
    } catch (e) {
      await fsp.rm(tmp, { force: true });
      await fsp.rm(file, { force: true });
      throw e instanceof UpdateError || e instanceof CatalogError ? e : new UpdateError('network', e.message);
    }
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
  // install script, then the backend restarted.
  async apply(job, staged) {
    const self = this;
    job.state = 'backing-up';
    const backup = self.plugin.backupCreate(self.backupName(staged.version), { automatic: true });
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

  backupName(version) {
    const base = 'before-' + version;
    const names = (this.plugin.backupList() || []).map(function (b) { return b.name; });
    if (names.indexOf(base) === -1) return base;
    const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15).replace('T', '-');
    return base + '-' + stamp;
  }
}

module.exports = { Updater: Updater, UpdateError: UpdateError, compareVersions: compareVersions, parseRelease: parseRelease, RELEASES_URL: RELEASES_URL, KEEP_AUTOMATIC_BACKUPS: KEEP_AUTOMATIC_BACKUPS };
