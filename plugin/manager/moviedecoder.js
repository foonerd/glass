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
// to start; nothing is fetched by itself. The same archive brought by hand
// (`installFile`) goes in the same way, held to the manifest it carries
// and to being the one file this player takes: the lock's name and
// version for the player's architecture.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const { get, downloadChecked, CatalogError } = require('./catalog');
const { TarGz } = require('./tarball');
const { verifySums, digestInSums, verifyArchive, SUMS_NAME, SIG_NAME, MAX_SUMS_BYTES, MAX_SIG_BYTES } = require('./signing');

const MAX_ARCHIVE_BYTES = 64 * 1024 * 1024;
const MAX_TEXT_BYTES = 256 * 1024;
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
  // key, ours unless given), log (a function taking a line); waits and
  // sleep stand in for a broken download's waits in tests.
  constructor(options) {
    options = options || {};
    this.dir = options.dir;
    this.lock = typeof options.lock === 'string' ? parseLock(options.lock) : options.lock;
    this.archOf = options.arch || function () { return ''; };
    this.fetch = options.fetch || get;
    this.pem = options.pem;
    this.log = options.log || function () {};
    this.waits = options.waits;
    this.sleep = options.sleep;
    this.job = null;
  }

  programPath(index) {
    return path.join(this.dir, 'bin', this.lock.programs[index]);
  }

  // The one file this player takes, as the release publishes it, and
  // where: null while the architecture is not known.
  wantedFile() {
    const arch = this.archOf();
    if (!/^(arm|armv7|armv8|x64)$/.test(arch)) return null;
    const lock = this.lock;
    const name = lock.name + '-' + lock.version + '-' + arch + '.tar.gz';
    return {
      name: name,
      arch: arch,
      url: 'https://github.com/' + lock.repository + '/releases/download/v' + lock.version + '/' + name,
      page: 'https://github.com/' + lock.repository + '/releases/tag/v' + lock.version
    };
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
      file: this.wantedFile(),
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
    // The folder the archive comes down into is kept between tries, so a
    // download that broke for good is carried on by the next one; it holds
    // the part of the archive wanted now and nothing else, and goes once
    // the component is in place.
    const download = this.dir + '.download';
    await fsp.mkdir(download, { recursive: true });
    for (const name of await fsp.readdir(download)) {
      if (name !== asset + '.part') await fsp.rm(path.join(download, name), { recursive: true, force: true });
    }
    try {
      await this.take(job, base, asset, stated, arch, download);
    } catch (e) {
      // A part the network left stays for the next try; an archive that
      // came whole and was refused goes, and the folder with it when
      // nothing is left in it.
      await fsp.rm(path.join(download, asset), { force: true });
      try { await fsp.rmdir(download); } catch (x) { /* the part stays */ }
      throw e;
    }
    await fsp.rm(download, { recursive: true, force: true });
    this.log('movie decoder: ' + lock.name + ' ' + lock.version + ' for ' + arch + ' in place');
  }

  // The archive downloaded into the folder, held to the digest the signed
  // sums state and to the manifest it carries, its programs put in place.
  async take(job, base, asset, stated, arch, download) {
    const lock = this.lock;
    const file = path.join(download, asset);
    const log = this.log;
    job.state = 'downloading';
    job.progress = { done: 0, total: 0 };
    try {
      // Carried on from where it stood when the connection breaks, and
      // from the part an earlier try left; held to the digest the signed
      // sums state before anything is read from it.
      const got = await downloadChecked(base + asset, file, {
        fetch: this.fetch,
        sha256: stated,
        limit: MAX_ARCHIVE_BYTES,
        waits: this.waits,
        sleep: this.sleep,
        onProgress: function (done, total) { job.progress = { done: done, total: total || 0 }; },
        onVerify: function () { job.state = 'verifying'; },
        told: function (e, said) {
          log('movie decoder: the download of ' + asset + ' broke at ' + said.at + ' bytes (' + e.message + '); carried on in ' + (said.wait / 1000) + ' s, attempt ' + said.attempt);
        },
        dropped: function (bytes) {
          log('movie decoder: the ' + bytes + ' bytes an earlier try had left of ' + asset + ' were not of this file: dropped, and the archive brought down again');
        }
      });
      if (got.from > 0) log('movie decoder: ' + asset + ' carried on from the ' + got.from + ' bytes an earlier try had left');
    } catch (e) {
      if (e instanceof CatalogError && e.code === 'checksum') throw new DecoderError('checksum', asset + ' is not the file the signed sums name');
      throw e;
    }
    job.state = 'verifying';
    const tar = await TarGz.open(file);
    try {
      // The archive held to the manifest it carries, every file of it.
      const held = await verifyArchive(tar, this.pem);
      if (!held) throw new DecoderError('signature', asset + ' carries no signed manifest');
      await this.place(job, tar, asset, arch);
    } finally {
      await tar.close();
    }
  }

  // An open, verified archive's programs, licences and facts put in place
  // of the component: staged beside it, swapped in, the one before gone.
  async place(job, tar, asset, arch) {
    const lock = this.lock;
    try {
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
    }
  }

  // The component from a file brought by hand, for a player that cannot
  // fetch it: one job at a time, as a fetch, and refused while one runs.
  // The job is the answer: `installed`, or `failed` with why.
  async installFile(file) {
    if (this.job && !this.job.done) throw new DecoderError('busy', 'a fetch is under way');
    const job = { state: 'verifying', progress: null, error: null, done: false };
    this.job = job;
    try {
      await this.runFile(job, file);
      job.state = 'installed';
    } catch (e) {
      job.state = 'failed';
      job.error = { code: e.code || 'failed', message: e.message || String(e) };
      this.log('movie decoder: a file refused: ' + job.error.code + ': ' + job.error.message);
    } finally {
      job.done = true;
    }
    return job;
  }

  // The file told from every other, in this order: a gzipped tar at all;
  // this component's by the top folder it unpacks to (name, version,
  // architecture); the release as published by the manifest and the
  // signature it carries, with no network; for this player's
  // architecture; of the version this Glass names. Only then is anything
  // put in place. Each refusal names the file the player takes.
  async runFile(job, file) {
    const lock = this.lock;
    const wanted = this.wantedFile();
    if (!wanted) throw new DecoderError('arch', 'the player\'s architecture is not known: ' + JSON.stringify(this.archOf()));
    const takes = '; this player takes ' + wanted.name;
    let tar;
    try {
      tar = await TarGz.open(file);
    } catch (e) {
      throw new DecoderError('not-archive', 'the file is not a gzipped tar archive' + takes);
    }
    try {
      const tops = new Set(tar.entries.filter(function (e) { return e.name.indexOf('/') !== -1; }).map(function (e) { return e.name.slice(0, e.name.indexOf('/')); }));
      const said = tops.size === 1 ? /^([a-z][a-z0-9]*)-(\d+\.\d+\.\d+)-([a-z0-9][a-z0-9-]*)$/.exec(Array.from(tops)[0]) : null;
      if (!said || said[1] !== lock.name) throw new DecoderError('not-decoder', 'the file is not an archive of ' + lock.name + takes);
      let held;
      try {
        held = await verifyArchive(tar, this.pem);
      } catch (e) {
        throw new DecoderError('signature', (e && e.message ? e.message : 'the archive\'s signature does not hold') + '; nothing was installed');
      }
      if (!held) throw new DecoderError('signature', 'the archive carries no signed manifest; it is not the release as published');
      if (said[3] !== wanted.arch) throw new DecoderError('other-arch', 'the file is ' + lock.name + ' for ' + said[3] + takes);
      if (said[2] !== lock.version) throw new DecoderError('other-version', 'the file is ' + lock.name + ' ' + said[2] + takes);
      await this.place(job, tar, wanted.name, wanted.arch);
    } finally {
      await tar.close();
    }
    // A part of the same archive the network left is moot now.
    await fsp.rm(this.dir + '.download', { recursive: true, force: true });
    this.log('movie decoder: ' + lock.name + ' ' + lock.version + ' for ' + wanted.arch + ' in place, from a file');
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

// Whether the player should fetch its decoder by itself now: the picture
// wanted (the switch on and the profile allowing it), the component missing
// or older than the lock names, no fetch under way, and the last try (a
// failure) an hour or more ago. Pure, for the plugin's clock.
const RETRY_MS = 60 * 60 * 1000;
function shouldFetch(facts, wanted, nowMs, lastTryMs) {
  if (!wanted || !facts) return false;
  if (facts.state !== 'missing' && facts.state !== 'older') return false;
  if (facts.job && facts.job.state !== 'installed' && facts.job.state !== 'failed') return false;
  if (lastTryMs && nowMs - lastTryMs < RETRY_MS) return false;
  return true;
}

module.exports = { Decoder, DecoderError, parseLock, compare, DISTRIBUTION, shouldFetch, RETRY_MS };
