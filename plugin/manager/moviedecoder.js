'use strict';
// evovpeg, the movie's decoder, as a component the Manager fetches and
// keeps: the release the plugin's lock names (components/evovpeg.lock:
// NAME, VERSION, REPOSITORY, PROGRAMS), one archive per architecture,
// the release's SHA256SUMS held to its signature by the evo release key,
// the archive to the sums, and the archive's files to the signed MANIFEST
// it carries; then the programs, their licences and the build's facts
// under the player's own folder, and the one before is gone. Until the
// component is here, the display runs the distribution's ffmpeg and
// ffprobe by name where the image has them. The fetching is the caller's
// to start; nothing is fetched by itself.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const crypto = require('crypto');
const { get } = require('./catalog');
const { TarGz } = require('./tarball');
const { verifySums, digestInSums, verifyArchive, SUMS_NAME, SIG_NAME, MAX_SUMS_BYTES, MAX_SIG_BYTES } = require('./signing');

const MAX_ARCHIVE_BYTES = 64 * 1024 * 1024;
const MAX_TEXT_BYTES = 256 * 1024;
const DOWNLOAD_TIMEOUT_MS = 10 * 60 * 1000;
// What is kept from the archive beside the programs: the licences and the
// build's facts, as the release carries them.
const KEPT = /^(LICENSE|README\.md|build\.json|licenses\/[^/]+)$/;
// The distribution's programs, where the image has them.
const DISTRIBUTION = {
  decoder: ['/usr/bin/ffmpeg', '/usr/local/bin/ffmpeg'],
  prober: ['/usr/bin/ffprobe', '/usr/local/bin/ffprobe']
};

class DecoderError extends Error {
  constructor(code, message, data) {
    super(message || code);
    this.code = code;
    if (data) this.data = data;
  }
}

// The lock's lines: NAME, VERSION, REPOSITORY and PROGRAMS (the decoder
// first, the prober second), `#` lines and blank ones passed over.
function parseLock(text) {
  const out = {};
  String(text || '').split(/\r?\n/).forEach(function (line) {
    const m = /^([A-Z]+)=(.*)$/.exec(line.trim());
    if (m) out[m[1]] = m[2].trim();
  });
  const programs = String(out.PROGRAMS || '').split(/\s+/).filter(Boolean);
  if (!out.NAME || !/^[a-z][a-z0-9-]*$/.test(out.NAME)) throw new DecoderError('bad-lock', 'the lock names no component');
  if (!out.VERSION || !/^\d+\.\d+\.\d+$/.test(out.VERSION)) throw new DecoderError('bad-lock', 'the lock names no version');
  if (!out.REPOSITORY || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(out.REPOSITORY)) throw new DecoderError('bad-lock', 'the lock names no repository');
  if (programs.length !== 2 || !programs.every(function (p) { return /^[a-z][a-z0-9-]*$/.test(p); })) throw new DecoderError('bad-lock', 'the lock must name the decoder and the prober');
  return { name: out.NAME, version: out.VERSION, repository: out.REPOSITORY, programs: programs };
}

// The first of the paths that is an executable file, or null.
function firstProgram(paths) {
  for (const p of paths) {
    try {
      fs.accessSync(p, fs.constants.X_OK);
      if (fs.statSync(p).isFile()) return p;
    } catch (e) { /* not this one */ }
  }
  return null;
}

class Decoder {
  // options: dir (the component's folder), lock (the lock's text or its
  // parsed form), arch (a function naming the player's architecture),
  // fetch (the HTTP get, the Manager's unless a test gives one), pem (the
  // key, ours unless given), log (a function taking a line).
  constructor(options) {
    options = options || {};
    this.dir = options.dir;
    this.lock = typeof options.lock === 'string' ? parseLock(options.lock) : options.lock;
    this.archOf = options.arch || function () { return ''; };
    this.fetch = options.fetch || get;
    this.pem = options.pem;
    this.log = options.log || function () {};
    this.job = null;
  }

  programPath(index) {
    return path.join(this.dir, 'bin', this.lock.programs[index]);
  }

  // The component as installed: its version and when, or null.
  installed() {
    try {
      const version = fs.readFileSync(path.join(this.dir, 'VERSION'), 'utf8').trim();
      if (!/^\d+\.\d+\.\d+$/.test(version)) return null;
      const have = this.lock.programs.every(function (p) { return firstProgram([path.join(this.dir, 'bin', p)]); }, this);
      if (!have) return null;
      return { version: version, at: fs.statSync(path.join(this.dir, 'VERSION')).mtime.toISOString() };
    } catch (e) {
      return null;
    }
  }

  // Which programs serve the display: the component's, else the
  // distribution's, else none; and how the component stands against the
  // lock (installed, missing, older, newer) with the job under way if one.
  facts() {
    const installed = this.installed();
    let state = 'missing';
    if (installed) {
      state = installed.version === this.lock.version ? 'installed'
        : compare(installed.version, this.lock.version) < 0 ? 'older' : 'newer';
    }
    let programs = null;
    let source = 'none';
    if (installed) {
      programs = { decoder: this.programPath(0), prober: this.programPath(1) };
      source = 'component';
    } else {
      const decoder = firstProgram(DISTRIBUTION.decoder);
      const prober = firstProgram(DISTRIBUTION.prober);
      if (decoder && prober) {
        programs = { decoder: decoder, prober: prober };
        source = 'distribution';
      }
    }
    return {
      wanted: { name: this.lock.name, version: this.lock.version, repository: this.lock.repository },
      installed: installed,
      state: state,
      source: source,
      programs: programs,
      job: this.job ? { state: this.job.state, progress: this.job.progress, error: this.job.error } : null
    };
  }

  // The release's file by name, up to `max` bytes, as a Buffer.
  async text(base, name, max) {
    const got = await this.fetch(base + name, { timeout: 60 * 1000, limit: max });
    if (!got || !got.body) throw new DecoderError('fetch', name + ' did not arrive');
    return got.body;
  }

  // The component fetched, verified and put in place. One job at a time;
  // a second call while one runs answers the running job.
  async install() {
    if (this.job && !this.job.done) return this.job;
    const job = { state: 'starting', progress: null, error: null, done: false };
    this.job = job;
    try {
      await this.run(job);
      job.state = 'installed';
    } catch (e) {
      job.state = 'failed';
      job.error = { code: e.code || 'failed', message: e.message || String(e) };
      this.log('movie decoder: ' + job.error.code + ': ' + job.error.message);
    } finally {
      job.done = true;
    }
    return job;
  }

  async run(job) {
    const arch = this.archOf();
    if (!/^(arm|armv7|armv8|x64)$/.test(arch)) throw new DecoderError('arch', 'the player\'s architecture is not known: ' + JSON.stringify(arch));
    const lock = this.lock;
    const base = 'https://github.com/' + lock.repository + '/releases/download/v' + lock.version + '/';
    const asset = lock.name + '-' + lock.version + '-' + arch + '.tar.gz';
    job.state = 'checking';
    const sums = await this.text(base, SUMS_NAME, MAX_SUMS_BYTES);
    const sig = await this.text(base, SIG_NAME, MAX_SIG_BYTES);
    if (!verifySums(sums, sig, this.pem)) throw new DecoderError('signature', 'the release\'s sums are not signed with the evo release key');
    const stated = digestInSums(sums, asset);
    if (!stated) throw new DecoderError('no-asset', 'the release names no ' + asset);
    const download = this.dir + '.download';
    await fsp.rm(download, { recursive: true, force: true });
    await fsp.mkdir(download, { recursive: true });
    try {
      await this.take(job, base, asset, stated, arch, download);
    } finally {
      await fsp.rm(download, { recursive: true, force: true });
    }
    this.log('movie decoder: ' + lock.name + ' ' + lock.version + ' for ' + arch + ' in place');
  }

  // The archive downloaded into the folder, held to the digest the signed
  // sums state and to the manifest it carries, its programs put in place.
  async take(job, base, asset, stated, arch, download) {
    const lock = this.lock;
    const file = path.join(download, asset);
    const hash = crypto.createHash('sha256');
    const out = fs.createWriteStream(file);
    let received = 0;
    job.state = 'downloading';
    job.progress = { done: 0, total: 0 };
    await new Promise(function (resolve, reject) {
      out.on('error', reject);
      this.fetch(base + asset, {
        timeout: DOWNLOAD_TIMEOUT_MS,
        limit: MAX_ARCHIVE_BYTES,
        sink: function (chunk, total, length) {
          hash.update(chunk);
          received = total;
          out.write(chunk);
          job.progress = { done: received, total: length || 0 };
        }
      }).then(function () { out.end(resolve); }, function (e) { out.destroy(); reject(e); });
    }.bind(this));
    job.state = 'verifying';
    if (hash.digest('hex') !== stated) throw new DecoderError('checksum', asset + ' is not the file the signed sums name');
    const tar = await TarGz.open(file);
    try {
      // The archive held to the manifest it carries, every file of it.
      const held = await verifyArchive(tar, this.pem);
      if (!held) throw new DecoderError('signature', asset + ' carries no signed manifest');
      const top = lock.name + '-' + lock.version + '-' + arch + '/';
      const entry = function (name) { return tar.entries.find(function (e) { return e.isRegular && e.name === top + name; }); };
      const staging = this.dir + '.new';
      await fsp.rm(staging, { recursive: true, force: true });
      const write = async function (name, bytes, mode) {
        const to = path.join(staging, name);
        await fsp.mkdir(path.dirname(to), { recursive: true });
        await fsp.writeFile(to, bytes, { mode: mode });
        await fsp.chmod(to, mode);
      };
      for (const program of lock.programs) {
        const found = entry('bin/' + program);
        if (!found) throw new DecoderError('no-program', asset + ' holds no bin/' + program);
        await write('bin/' + program, await tar.read(found), 0o755);
      }
      for (const e of tar.entries) {
        if (!e.isRegular || !e.name.startsWith(top)) continue;
        const rel = e.name.slice(top.length);
        if (KEPT.test(rel) && e.size <= MAX_TEXT_BYTES) await write(rel, await tar.read(e), 0o644);
      }
      await write('VERSION', lock.version + '\n', 0o644);
      job.state = 'installing';
      const prev = this.dir + '.prev';
      await fsp.rm(prev, { recursive: true, force: true });
      if (fs.existsSync(this.dir)) await fsp.rename(this.dir, prev);
      await fsp.rename(staging, this.dir);
      await fsp.rm(prev, { recursive: true, force: true });
    } catch (e) {
      // Nothing half-written left: the staging folder goes with the fault.
      await fsp.rm(this.dir + '.new', { recursive: true, force: true });
      throw e;
    } finally {
      await tar.close();
    }
  }

  // The component gone; the distribution's programs serve again, where
  // the image has them.
  async remove() {
    if (this.job && !this.job.done) throw new DecoderError('busy', 'a fetch is under way');
    await fsp.rm(this.dir, { recursive: true, force: true });
    await fsp.rm(this.dir + '.new', { recursive: true, force: true });
    await fsp.rm(this.dir + '.prev', { recursive: true, force: true });
    await fsp.rm(this.dir + '.download', { recursive: true, force: true });
  }
}

// Versions compared by their three numbers.
function compare(a, b) {
  const x = String(a).split('.').map(Number);
  const y = String(b).split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) < (y[i] || 0) ? -1 : 1;
  }
  return 0;
}

module.exports = { Decoder, DecoderError, parseLock, compare, DISTRIBUTION };
