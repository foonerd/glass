'use strict';
// The player's shelf for remotes. A release archive for a remote's
// machine, brought by hand, is kept here and offered on the network in
// the shape GitHub's releases have, so a remote that cannot reach GitHub
// upgrades from its player: a remote looks at its player first and at
// GitHub second. Behind the same switch as install from a file; off, the
// shelf refuses uploads and serves nothing. Every archive is held to the
// signature it carries inside before it is kept, and is named by what it
// says it is, not by the name it came with. The shelf holds one archive
// per product and platform, the newer replacing the older as it lands,
// and a ceiling in megabytes bounds the whole; a player's storage is
// small.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const crypto = require('crypto');
const { Zip } = require('./zip');
const { TarGz, isZipFile } = require('./tarball');
const signing = require('./signing');

const INDEX = 'shelf.json';
// A remote's archive weighs a few megabytes; the bound is the remote's own.
const MAX_ARCHIVE_BYTES = 64 * 1024 * 1024;
// The ceiling for the whole shelf: a full set of both products for every
// platform weighs about fifty megabytes at the time of writing.
const DEFAULT_CEILING_MB = 128;
const MIN_CEILING_MB = 16;
const MAX_CEILING_MB = 2048;

// The platforms a remote runs on, by the folder its archive is laid out as.
const PLATFORMS = {
  'x64': 'Linux x86_64',
  'armv8': 'Linux ARM 64-bit',
  'armv7': 'Linux ARM 32-bit',
  'arm': 'Linux ARM 32-bit, soft float',
  'windows-x64': 'Windows x64'
};

class ShelfError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

// A ceiling as given, held inside the bounds; the default for nothing.
function ceilingBounded(mb) {
  const n = Math.round(Number(mb));
  if (!Number.isFinite(n) || n <= 0) return DEFAULT_CEILING_MB;
  return Math.min(MAX_CEILING_MB, Math.max(MIN_CEILING_MB, n));
}

// The folder a remote's archive is laid out as, `<asset><version>-<arch>`,
// read: the product by the asset, the version and the platform. Null where
// it is not one.
function parseTop(top) {
  const m = /^(glass-evo-|glass-)(\d+\.\d+\.\d+)-([a-z0-9-]+)$/.exec(String(top || ''));
  if (!m || !PLATFORMS[m[3]]) return null;
  return { asset: m[1], product: m[1] === 'glass-' ? 'Glass' : 'glass-evo', binary: m[1] === 'glass-' ? 'glass' : 'glass-evo', version: m[2], platform: m[3] };
}

// The archive's name in a release, as its platform packs it.
function archiveName(what) {
  return what.asset + what.version + '-' + what.platform + (what.platform.startsWith('windows') ? '.zip' : '.tar.gz');
}

function platformName(platform) {
  return PLATFORMS[platform] || platform;
}

function numbers(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(version || ''));
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

// Later first; versions that are none last.
function byVersionDesc(a, b) {
  const x = numbers(a), y = numbers(b);
  if (!x || !y) return x ? -1 : y ? 1 : 0;
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return y[i] - x[i];
  return 0;
}

// A file's SHA-256, streamed.
function digestOf(file) {
  return new Promise(function (resolve, reject) {
    const hash = crypto.createHash('sha256');
    fs.createReadStream(file).on('data', function (c) { hash.update(c); }).on('error', reject).on('end', function () { resolve(hash.digest('hex')); });
  });
}

// An archive examined for the shelf: held to the signature it carries
// inside with `pem` (the project's key unless given), then what it is by
// the one folder its files lie under, with the product's binary there for
// the platform. The player's own zips, an Android package and anything
// else are refused as not a remote's archive.
async function examine(file, pem) {
  const archive = (await isZipFile(file)) ? await Zip.open(file) : await TarGz.open(file);
  try {
    const held = await signing.verifyArchive(archive, pem);
    if (!held) throw new ShelfError('unsigned', 'the archive carries no signature inside; a release published before signing cannot go on the shelf');
    const tops = new Set();
    archive.entries.forEach(function (e) {
      if (!e.isRegular || e.name === signing.MANIFEST_NAME || e.name === signing.MANIFEST_SIG_NAME) return;
      tops.add(e.name.split('/')[0]);
    });
    const top = tops.size === 1 ? Array.from(tops)[0] : '';
    const what = parseTop(top);
    if (!what) throw new ShelfError('not-remote-archive', 'the archive is not a remote\'s release archive (glass-<version>-<arch>.tar.gz, the windows-x64 zip, or glass-evo\'s)');
    const binary = archive.entries.some(function (e) {
      if (!e.isRegular || !e.name.startsWith(top + '/bin/')) return false;
      const base = e.name.slice(e.name.lastIndexOf('/') + 1);
      return base === what.binary || base === what.binary + '.exe';
    });
    if (!binary) throw new ShelfError('not-remote-archive', 'the archive holds no ' + what.product + ' binary under ' + top + '/bin');
    return Object.assign(what, { files: held.files });
  } finally {
    await archive.close();
  }
}

// The archives of a product in the shape GitHub's releases list has, one
// release per version, newest first, the assets served at `base`
// (`http://<host>:<port>`, as the remote reached the player). What a
// remote reads with its own release code, unchanged.
function releasesOf(archives, product, base) {
  // A remote asks by its asset's stem, `glass` or `glass-evo`; the page
  // and the index say `Glass`. Either names the product.
  const wanted = String(product || '').toLowerCase() === 'glass' ? 'Glass' : String(product || '');
  const byVersion = new Map();
  archives.filter(function (a) { return a.product === wanted; }).forEach(function (a) {
    if (!byVersion.has(a.version)) byVersion.set(a.version, []);
    byVersion.get(a.version).push({
      name: a.name,
      size: a.bytes,
      browser_download_url: base + '/api/shelf/files/' + encodeURIComponent(a.name),
      digest: 'sha256:' + a.sha256
    });
  });
  return Array.from(byVersion.keys()).sort(byVersionDesc).map(function (version) {
    return { tag_name: 'v' + version, name: wanted + ' ' + version, draft: false, prerelease: false, html_url: base + '/manage', assets: byVersion.get(version) };
  });
}

class Shelf {
  // dir: where the archives and the index live; ceilingMb: a function
  // answering the ceiling set; publicKey: another PEM for tests; logger.
  constructor(options) {
    this.dir = options.dir;
    this.ceilingMb = options.ceilingMb || function () { return DEFAULT_CEILING_MB; };
    this.publicKey = options.publicKey || null;
    this.logger = options.logger || console;
  }

  // The archives as the index names them and the folder holds them: one
  // whose file is gone is dropped from the index.
  async load() {
    let listed = [];
    try {
      const saved = JSON.parse(await fsp.readFile(path.join(this.dir, INDEX), 'utf8'));
      if (saved && Array.isArray(saved.archives)) listed = saved.archives;
    } catch (e) { /* no index yet */ }
    const kept = [];
    for (const a of listed) {
      if (!a || typeof a.name !== 'string' || a.name.includes('/') || a.name.includes('..')) continue;
      try {
        const stat = await fsp.stat(path.join(this.dir, a.name));
        if (stat.isFile() && stat.size === a.bytes) kept.push(a);
      } catch (e) { /* gone */ }
    }
    if (kept.length !== listed.length) await this.save(kept);
    return kept;
  }

  async save(archives) {
    await fsp.mkdir(this.dir, { recursive: true });
    const tmp = path.join(this.dir, INDEX + '.part');
    await fsp.writeFile(tmp, JSON.stringify({ archives: archives }, null, 2));
    await fsp.rename(tmp, path.join(this.dir, INDEX));
  }

  usedBytes(archives) {
    return archives.reduce(function (sum, a) { return sum + (a.bytes || 0); }, 0);
  }

  // What the page shows.
  async view() {
    const archives = await this.load();
    const ceiling = ceilingBounded(this.ceilingMb());
    return {
      ceilingMb: ceiling,
      usedBytes: this.usedBytes(archives),
      archives: archives.map(function (a) {
        return { name: a.name, product: a.product, version: a.version, platform: a.platform, platformName: platformName(a.platform), bytes: a.bytes, added: a.added };
      })
    };
  }

  // An archive received into `file`, examined and placed: the one of the
  // same product and platform goes as it lands, and the ceiling holds the
  // whole. The file is consumed, kept under the archive's own name, or
  // removed when refused.
  async place(file) {
    try {
      const what = await examine(file, this.publicKey);
      const name = archiveName(what);
      const stat = await fsp.stat(file);
      const archives = await this.load();
      const kept = archives.filter(function (a) { return !(a.product === what.product && a.platform === what.platform); });
      const dropped = archives.filter(function (a) { return a.product === what.product && a.platform === what.platform; });
      const ceiling = ceilingBounded(this.ceilingMb());
      const used = this.usedBytes(kept);
      if (used + stat.size > ceiling * 1024 * 1024) {
        const mb = function (n) { return (n / 1048576).toFixed(1); };
        throw new ShelfError('shelf-full', 'the shelf holds ' + mb(used) + ' MB of its ' + ceiling + ' MB and this archive of ' + mb(stat.size) + ' MB does not fit; remove an archive or raise the ceiling');
      }
      const entry = { name: name, product: what.product, version: what.version, platform: what.platform, bytes: stat.size, sha256: await digestOf(file), files: what.files, added: new Date().toISOString() };
      await fsp.mkdir(this.dir, { recursive: true });
      await fsp.rename(file, path.join(this.dir, name));
      for (const a of dropped) {
        if (a.name !== name) await fsp.rm(path.join(this.dir, a.name), { force: true });
      }
      kept.push(entry);
      await this.save(kept);
      this.logger.info('glass: manager: shelf: ' + what.product + ' ' + what.version + ' for ' + platformName(what.platform) + ' kept as ' + name + ' (' + what.files + ' files verified)' + (dropped.length ? ', ' + dropped.map(function (a) { return a.name; }).join(', ') + ' dropped' : ''));
      return Object.assign({ platformName: platformName(what.platform) }, entry);
    } catch (e) {
      await fsp.rm(file, { force: true });
      throw e;
    }
  }

  async remove(name) {
    const archives = await this.load();
    const gone = archives.find(function (a) { return a.name === name; });
    if (!gone) return false;
    await fsp.rm(path.join(this.dir, gone.name), { force: true });
    await this.save(archives.filter(function (a) { return a.name !== name; }));
    this.logger.info('glass: manager: shelf: ' + name + ' removed');
    return true;
  }

  // The path of an archive the index names, with its size; null for any other name.
  async file(name) {
    const archives = await this.load();
    const found = archives.find(function (a) { return a.name === name; });
    return found ? { path: path.join(this.dir, found.name), bytes: found.bytes } : null;
  }

  async releases(product, base) {
    return releasesOf(await this.load(), product, base);
  }
}

module.exports = { Shelf, ShelfError, examine, parseTop, archiveName, platformName, releasesOf, ceilingBounded, MAX_ARCHIVE_BYTES, DEFAULT_CEILING_MB, MIN_CEILING_MB, MAX_CEILING_MB, PLATFORMS };
