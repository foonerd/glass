'use strict';
// glass-evo as a component the Manager fetches and keeps. The latest
// release is read from GitHub and its zip checked against the digest the
// release carries; it is unpacked beside the installed one, the manifest
// read, this player's binary checked against the manifest's digest, the
// looks and the built-in look taken with it; then it takes the installed
// one's place, and the one before is kept for the way back.
//
// The two name the least of each other they work with: the component's
// manifest the least Glass (requires.glass), Glass's package the least
// glass-evo (glassEvo.least). A pair outside either is never installed,
// and a Glass about to be installed that needs a newer component has it
// installed first.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const crypto = require('crypto');
const { get } = require('./catalog');
const { Zip, safeName } = require('./zip');
const { compareVersions, parseRelease, fetchChecked } = require('./update');

const RELEASES_URL = 'https://api.github.com/repos/foonerd/glass-evo/releases/latest';
const ASSET = /^glass-evo-(\d+\.\d+\.\d+)\.zip$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const CHECK_TTL_MS = 24 * 60 * 60 * 1000;
const MAX_RELEASE_BYTES = 1024 * 1024;
const MAX_ZIP_BYTES = 128 * 1024 * 1024;
const MAX_TEXT_BYTES = 64 * 1024;
// A look in the component: a folder under themes/ with its text.
const LOOK = /^themes\/[^/]+\/face\.txt$/;

class ComponentError extends Error {
  constructor(code, message, data) {
    super(message || code);
    this.code = code;
    this.data = data || null;
  }
}

// The least glass-evo a Glass works with, from its package's text; null
// where it names none.
function leastOf(packageText) {
  try {
    const least = JSON.parse(packageText).glassEvo.least;
    return VERSION.test(String(least)) ? String(least) : null;
  } catch (e) {
    return null;
  }
}

// A manifest's text as what the Manager reads of it.
function manifestOf(text) {
  let m;
  try {
    m = JSON.parse(text);
  } catch (e) {
    throw new ComponentError('bad-manifest', 'the manifest is not JSON');
  }
  if (!m || m.name !== 'glass-evo' || !VERSION.test(String(m.version || '')) || !m.binaries || typeof m.binaries !== 'object') {
    throw new ComponentError('bad-manifest', 'the manifest does not describe glass-evo');
  }
  const needs = m.requires && VERSION.test(String(m.requires.glass || '')) ? String(m.requires.glass) : null;
  return { version: String(m.version), binaries: m.binaries, requires: needs };
}

// Why a component does not go with a Glass on this player, or null: older
// than that Glass works with, needing a newer Glass than that one, or with
// no binary for the player's architecture.
function misfit(manifest, glass, arch, least) {
  if (least && compareVersions(manifest.version, least) < 0) return { error: 'too-old', least: least };
  if (manifest.requires && compareVersions(glass, manifest.requires) < 0) return { error: 'needs-glass', needs: manifest.requires };
  const bin = manifest.binaries[arch];
  if (!bin || !safeName(String(bin.path || '')) || !/^[0-9a-f]{64}$/.test(String(bin.sha256 || ''))) return { error: 'no-binary', arch: arch };
  return null;
}

function misfitMessage(manifest, problem) {
  if (problem.error === 'too-old') return 'glass-evo ' + manifest.version + ' is older than this Glass works with (' + problem.least + ')';
  if (problem.error === 'needs-glass') return 'glass-evo ' + manifest.version + ' needs Glass ' + problem.needs + ' or later';
  return 'glass-evo ' + manifest.version + ' has no binary for this player (' + problem.arch + ')';
}

// The component in a folder: whether a manifest is there, its version,
// the least Glass it names, and whether this player's binary is there.
function installedAt(dir, arch) {
  try {
    const m = manifestOf(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf8'));
    const rel = m.binaries[arch] && m.binaries[arch].path;
    const bin = rel && safeName(String(rel)) ? path.join(dir, String(rel)) : null;
    const available = !!(bin && fs.existsSync(bin));
    return { installed: true, available: available, version: m.version, binary: available ? bin : null, arch: arch, requires: m.requires };
  } catch (e) {
    return { installed: false, available: false, version: null, binary: null, arch: arch, requires: null };
  }
}

// What a Glass about to be installed asks of the component that is here:
// nothing, a newer component first, or that it cannot be, the component
// needing a newer Glass than that one (a step back of Glass).
function pairPlan(installed, target) {
  if (!installed || !installed.installed) return { action: 'none' };
  if (installed.requires && compareVersions(target.version, installed.requires) < 0) return { action: 'refuse', needs: installed.requires };
  if (target.least && compareVersions(installed.version, target.least) < 0) return { action: 'update', least: target.least };
  return { action: 'none' };
}

class Component {
  // dir: where the component lives; beside it <dir>.new while one is
  // unpacked and <dir>.prev, the one before. stateDir: where the last
  // look at the releases is kept and a download lands. glass: this
  // Glass's version; least: the least glass-evo it works with; arch: the
  // player's architecture, asked when needed.
  constructor(options) {
    this.dir = options.dir;
    this.stateDir = options.stateDir;
    this.glass = options.glass;
    this.least = options.least || null;
    this.arch = options.arch;
    this.logger = options.logger || console;
    this.releasesUrl = options.releasesUrl || RELEASES_URL;
    this.fetch = options.fetch || get;
    this.latest = null;
    this.checkedAt = null;
  }

  async init() {
    await fsp.mkdir(this.stateDir, { recursive: true });
    try {
      const saved = JSON.parse(await fsp.readFile(path.join(this.stateDir, 'latest.json'), 'utf8'));
      if (saved && saved.latest && saved.latest.version) {
        this.latest = saved.latest;
        this.checkedAt = saved.checkedAt || null;
      }
    } catch (e) { /* not looked yet */ }
    // One left half unpacked by a start in the middle is not a component.
    await fsp.rm(this.dir + '.new', { recursive: true, force: true });
  }

  installed() {
    return installedAt(this.dir, this.arch());
  }

  previous() {
    const p = installedAt(this.dir + '.prev', this.arch());
    return p.installed ? { version: p.version, requires: p.requires } : null;
  }

  view() {
    const now = this.installed();
    const latest = this.latest;
    const newer = !!latest && (!now.installed || compareVersions(latest.version, now.version) > 0);
    const behind = !!latest && !!this.least && compareVersions(latest.version, this.least) < 0;
    return {
      installed: now.installed ? { version: now.version, available: now.available, requires: now.requires } : null,
      glass: this.glass,
      least: this.least,
      // Here, and older than this Glass works with.
      outdated: !!(now.installed && this.least && compareVersions(now.version, this.least) < 0),
      latest: latest,
      checkedAt: this.checkedAt,
      // Something to get: a release newer than what is here that this Glass works with.
      available: newer && !behind,
      // The latest release is itself older than this Glass works with.
      behind: behind,
      previous: this.previous()
    };
  }

  // The latest release, from GitHub when the last look is older than a day or `force`.
  async check(force) {
    const fresh = this.latest && this.checkedAt && (Date.now() - new Date(this.checkedAt).getTime()) < CHECK_TTL_MS;
    if (!force && fresh) return this.view();
    const res = await this.fetch(this.releasesUrl, {
      headers: { accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
      limit: MAX_RELEASE_BYTES
    });
    let body;
    try {
      body = JSON.parse(res.body.toString('utf8'));
    } catch (e) {
      throw new ComponentError('bad-release', 'the release answer is not JSON');
    }
    this.latest = parseRelease(body, ASSET);
    this.checkedAt = new Date().toISOString();
    const file = path.join(this.stateDir, 'latest.json');
    await fsp.writeFile(file + '.tmp', JSON.stringify({ latest: this.latest, checkedAt: this.checkedAt }));
    await fsp.rename(file + '.tmp', file);
    return this.view();
  }

  // The latest release's zip, checked against the release's digest.
  async download(job) {
    const latest = this.latest;
    if (!latest) throw new ComponentError('no-release', 'no release known; check first');
    if (!latest.sha256) throw new ComponentError('no-digest', 'the release carries no checksum for its zip');
    if (!latest.bytes || latest.bytes > MAX_ZIP_BYTES) throw new ComponentError('bad-release', 'the release zip has an unusable size');
    const file = path.join(this.stateDir, 'glass-evo-' + latest.version + '.zip');
    await fetchChecked(latest, file, job, this.fetch);
    return file;
  }

  // Unpack a component's zip beside the installed one, checking as it
  // goes: the manifest, that it goes with the Glass in `target` on this
  // player, the binary against the manifest's digest. Only what this
  // player uses is written: the manifest, its binary, the looks, the
  // built-in look.
  async unpack(file, target, version) {
    const arch = this.arch();
    const staging = this.dir + '.new';
    await fsp.rm(staging, { recursive: true, force: true });
    const zip = await Zip.open(file);
    try {
      const entry = function (name) { return zip.entries.find(function (e) { return e.isRegular && e.name === name; }); };
      const found = entry('manifest.json');
      if (!found || found.size > MAX_TEXT_BYTES) throw new ComponentError('bad-manifest', 'the zip holds no manifest');
      const text = (await zip.read(found)).toString('utf8');
      const manifest = manifestOf(text);
      if (version && manifest.version !== version) throw new ComponentError('bad-manifest', 'the manifest says ' + manifest.version + ', the release ' + version);
      const problem = misfit(manifest, target.glass, arch, target.least);
      if (problem) throw new ComponentError(problem.error, misfitMessage(manifest, problem), problem);
      const bin = manifest.binaries[arch];
      const binary = entry(bin.path);
      if (!binary) throw new ComponentError('no-binary', 'the zip holds no ' + bin.path, { arch: arch });
      const data = await zip.read(binary);
      if (crypto.createHash('sha256').update(data).digest('hex') !== bin.sha256) throw new ComponentError('checksum', 'the binary does not match the manifest');
      const write = async function (name, bytes, mode) {
        const to = path.join(staging, name);
        await fsp.mkdir(path.dirname(to), { recursive: true });
        await fsp.writeFile(to, bytes, { mode: mode });
        // The mode as asked, whatever the process's mask.
        await fsp.chmod(to, mode);
      };
      await write('manifest.json', text, 0o644);
      await write(bin.path, data, 0o755);
      for (const e of zip.entries) {
        if (!e.isRegular || !safeName(e.name) || e.size > MAX_TEXT_BYTES) continue;
        if (e.name === 'face.txt' || LOOK.test(e.name)) await write(e.name, await zip.read(e), 0o644);
      }
      return manifest;
    } catch (e) {
      await fsp.rm(staging, { recursive: true, force: true });
      throw e;
    } finally {
      await zip.close();
    }
  }

  // The unpacked component in the installed one's place, the one before kept.
  async swapIn() {
    const prev = this.dir + '.prev';
    await fsp.rm(prev, { recursive: true, force: true });
    let had = false;
    try {
      await fsp.rename(this.dir, prev);
      had = true;
    } catch (e) {
      if (e.code !== 'ENOENT') throw e;
    }
    try {
      await fsp.rename(this.dir + '.new', this.dir);
    } catch (e) {
      if (had) await fsp.rename(prev, this.dir);
      throw e;
    }
  }

  // Get the latest release, or update to it. `target` is the Glass it must
  // go with, its version and the least glass-evo it works with: this
  // Glass, or the one about to be installed.
  async install(job, target) {
    const latest = this.latest;
    if (!latest) throw new ComponentError('no-release', 'no release known; check first');
    const now = this.installed();
    if (now.installed && compareVersions(latest.version, now.version) <= 0) throw new ComponentError('up-to-date', 'glass-evo ' + now.version + ' is the latest');
    const file = await this.download(job);
    try {
      const manifest = await this.unpack(file, target || { glass: this.glass, least: this.least }, latest.version);
      job.state = 'applying';
      await this.swapIn();
      this.logger.info('glass: manager component: glass-evo ' + (now.installed ? now.version + ' to ' : '') + manifest.version + ' installed');
      return { from: now.installed ? now.version : null, to: manifest.version };
    } finally {
      await fsp.rm(file, { force: true });
    }
  }

  // The one before back in place, and the one in place kept as the one
  // before. Only where it goes with this Glass.
  async rollback() {
    const arch = this.arch();
    const prev = this.dir + '.prev';
    let manifest;
    try {
      manifest = manifestOf(await fsp.readFile(path.join(prev, 'manifest.json'), 'utf8'));
    } catch (e) {
      throw new ComponentError('no-previous', 'no previous version is kept');
    }
    const problem = misfit(manifest, this.glass, arch, this.least);
    if (problem) throw new ComponentError(problem.error, misfitMessage(manifest, problem), problem);
    const now = this.installed();
    const aside = this.dir + '.swap';
    await fsp.rm(aside, { recursive: true, force: true });
    let had = false;
    try {
      await fsp.rename(this.dir, aside);
      had = true;
    } catch (e) {
      if (e.code !== 'ENOENT') throw e;
    }
    try {
      await fsp.rename(prev, this.dir);
    } catch (e) {
      if (had) await fsp.rename(aside, this.dir);
      throw e;
    }
    if (had) await fsp.rename(aside, prev);
    this.logger.info('glass: manager component: glass-evo back to ' + manifest.version + (now.installed ? ' from ' + now.version : ''));
    return { from: now.installed ? now.version : null, to: manifest.version };
  }

  async remove() {
    const now = this.installed();
    for (const suffix of ['', '.prev', '.new', '.swap']) await fsp.rm(this.dir + suffix, { recursive: true, force: true });
    if (now.installed) this.logger.info('glass: manager component: glass-evo ' + now.version + ' removed');
    return { removed: now.installed ? now.version : null };
  }
}

module.exports = { Component, ComponentError, RELEASES_URL, ASSET, leastOf, manifestOf, misfit, installedAt, pairPlan };
