'use strict';
// The theme catalog: the index glass_templates publishes, kept on the
// player with its ETag, the thumbnails it names fetched as they are looked
// at, and the zips downloaded and checked against the index's checksum
// before anything is unpacked. A zip that no longer matches the index as
// kept (the catalog moved on since) has the index fetched again and the
// download made once more. What was installed from the catalog is
// remembered by name and checksum, so an updated zip shows as an update.

const crypto = require('crypto');
const fs = require('fs');
const fsp = require('fs/promises');
const http = require('http');
const https = require('https');
const path = require('path');

const INDEX_URL = 'https://raw.githubusercontent.com/foonerd/glass_templates/main/catalog/index.json';
const INDEX_VERSION = 1;
const FETCH_TIMEOUT_MS = 20000;
// How long a download's connection may stand silent before it counts as
// broken and the download is carried on over a new one.
const STALL_TIMEOUT_MS = 60 * 1000;
const MAX_REDIRECTS = 5;
const MAX_INDEX_BYTES = 16 * 1024 * 1024;
const MAX_THUMB_BYTES = 2 * 1024 * 1024;
const MAX_ZIP_BYTES = 512 * 1024 * 1024;
const NAME = /^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,127}$/;
// The waits before a download whose connection broke is carried on: one
// for each try in a row that brought next to nothing, after which the
// download gives up (what it has is kept for the next time).
const RETRY_WAITS_MS = [2000, 4000, 8000];
// A try that broke counts as having brought something when it added this
// much; and no download is carried on more often than this.
const MIN_STEP_BYTES = 64 * 1024;
const MAX_RESUMES = 200;

class CatalogError extends Error {
  constructor(code, message) {
    super(message || code);
    this.code = code;
  }
}

// One HTTP GET with redirects and a timeout. `sink` takes each chunk; with
// no sink the body is collected up to `limit` bytes. With `range` (a count
// of bytes already held) the rest of the file is asked for from there, and
// an answer that is that rest (206) is taken as well as one that is the
// whole file (200, a server that knows no ranges); `onResponse(status,
// headers)` hears which before any of the body, and a throw from it ends
// the request with that error. `limit` bounds the whole file either way.
function get(url, options) {
  options = options || {};
  return new Promise(function (resolve, reject) {
    let redirects = 0;
    let finished = false;
    const fail = function (e) {
      if (finished) return;
      finished = true;
      reject(e);
    };
    const request = function (target) {
      let parsed;
      try {
        parsed = new URL(target);
      } catch (e) {
        return fail(new CatalogError('bad-url', 'not a URL: ' + target));
      }
      if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
        return fail(new CatalogError('bad-url', 'unsupported protocol in ' + target));
      }
      const client = parsed.protocol === 'https:' ? https : http;
      const from = Number.isInteger(options.range) && options.range > 0 ? options.range : 0;
      const headers = Object.assign({ 'user-agent': 'glass-manager' }, options.headers || {}, from ? { range: 'bytes=' + from + '-' } : {});
      const req = client.get(parsed, { headers: headers }, function (res) {
        const status = res.statusCode || 0;
        if (status >= 300 && status < 400 && res.headers.location) {
          res.resume();
          if (++redirects > MAX_REDIRECTS) return fail(new CatalogError('redirects', 'too many redirects'));
          return request(new URL(res.headers.location, parsed).toString());
        }
        if (status === 304) {
          res.resume();
          return resolve({ status: 304, headers: res.headers, body: null });
        }
        const rest = status === 206 && from > 0;
        if (status !== 200 && !rest) {
          res.resume();
          return fail(new CatalogError('http-' + status, 'HTTP ' + status + ' for ' + target));
        }
        // What is held already counts toward the limit when the answer is the rest.
        const held = rest ? from : 0;
        const length = parseInt(res.headers['content-length'], 10);
        if (options.limit && held + length > options.limit) {
          res.destroy();
          return fail(new CatalogError('too-large', 'response larger than allowed'));
        }
        if (options.onResponse) {
          try {
            options.onResponse(status, res.headers);
          } catch (e) {
            res.destroy();
            return fail(e instanceof CatalogError ? e : new CatalogError('network', e.message));
          }
        }
        const chunks = [];
        let received = 0;
        res.on('data', function (chunk) {
          received += chunk.length;
          if (options.limit && held + received > options.limit) {
            res.destroy();
            return fail(new CatalogError('too-large', 'response larger than allowed'));
          }
          if (options.sink) {
            options.sink(chunk, received, isNaN(length) ? 0 : length);
          } else {
            chunks.push(chunk);
          }
        });
        res.on('end', function () {
          if (finished) return;
          finished = true;
          resolve({ status: status, headers: res.headers, body: options.sink ? null : Buffer.concat(chunks), received: received });
        });
        // A connection that breaks in the body is the network's failure
        // as much as one that breaks before it.
        res.on('error', function (e) {
          fail(e instanceof CatalogError ? e : new CatalogError('network', e.message));
        });
      });
      req.setTimeout(options.timeout || FETCH_TIMEOUT_MS, function () {
        req.destroy(new CatalogError('timeout', 'no answer from ' + parsed.host));
      });
      req.on('error', function (e) {
        fail(e instanceof CatalogError ? e : new CatalogError('network', e.message));
      });
    };
    request(url);
  });
}

// Whether a failed fetch is worth making again: the connection broke or
// the server failed, as against an answer that would be the same next time.
function transient(e) {
  return e instanceof CatalogError && (e.code === 'network' || /^http-5\d\d$/.test(e.code));
}

// A file's SHA-256, read from the disk.
function sha256Of(file) {
  return new Promise(function (resolve, reject) {
    const hash = crypto.createHash('sha256');
    fs.createReadStream(file)
      .on('data', function (chunk) { hash.update(chunk); })
      .on('end', function () { resolve(hash.digest('hex')); })
      .on('error', reject);
  });
}

// The whole size an answer states for its file: the total of a rest's
// Content-Range, the Content-Length of a whole one; 0 where it says none.
function statedSize(status, headers) {
  if (status === 206) {
    const m = /^bytes\s+(\d+)-(\d+)\/(\d+)$/.exec(String((headers || {})['content-range'] || '').trim());
    return m ? parseInt(m[3], 10) : 0;
  }
  const length = parseInt((headers || {})['content-length'], 10);
  return isNaN(length) ? 0 : length;
}

// A file brought down whole into `file + '.part'`, carried on from where
// it stands whenever the connection breaks, stands silent or the server
// fails: what is on the disk is kept and the rest asked for (a server that
// knows no ranges sends the whole again, and the part starts over). A part
// found there from an earlier try is carried on the same way, so a
// download that gave up is not begun again from its first byte the next
// time. A try that broke having added next to nothing counts against the
// waits (`options.waits`, one for each such try in a row); when they run
// out the error stands and the part stays. Nothing here says the part is
// the right file: the caller holds it to a digest (`downloadChecked`).
// options: `fetch` in place of `get`; `bytes`, the file's size where it is
// known; `limit`, the most it may be; `timeout`, the silence allowed;
// `onProgress(done, total)`; `told(e, said)` before each carrying on, with
// `said` the bytes held (`at`), the size (`total`, 0 unknown), the try to
// come (`attempt`) and its wait in ms (`wait`); `sleep` for tests.
// Resolves with `bytes` (the size), `resumed` (how often it was carried
// on) and `from` (the bytes of a part found at the start).
async function download(url, file, options) {
  options = options || {};
  const fetch = options.fetch || get;
  const part = file + '.part';
  const waits = options.waits || RETRY_WAITS_MS;
  const sleep = options.sleep || function (ms) { return new Promise(function (resolve) { setTimeout(resolve, ms); }); };
  const sizeOf = async function () { try { return (await fsp.stat(part)).size; } catch (e) { return 0; } };
  const expected = Number.isInteger(options.bytes) && options.bytes > 0 ? options.bytes : 0;
  // One connection's worth: the answer's body onto the part, after what is
  // there when the answer is the rest, in its place when it is the whole.
  const once = function (have) {
    return new Promise(function (resolve, reject) {
      let out = null;
      let base = 0;
      let stated = 0;
      let fault = null;
      const open = function (status, headers) {
        if (status === 206) {
          const m = /^bytes\s+(\d+)-/.exec(String((headers || {})['content-range'] || '').trim());
          if (!m || parseInt(m[1], 10) !== have) throw new CatalogError('range', 'the server answered another part of the file than the one asked for');
        }
        base = status === 206 ? have : 0;
        stated = statedSize(status, headers);
        if (expected && stated && stated !== expected) throw new CatalogError('size', 'the file is ' + stated + ' bytes, expected ' + expected);
        out = fs.createWriteStream(part, { flags: base > 0 ? 'a' : 'w' });
        out.on('error', function (e) { fault = fault || e; });
      };
      const close = function (then) { if (out) out.end(then); else then(); };
      fetch(url, {
        timeout: options.timeout || STALL_TIMEOUT_MS,
        limit: options.limit,
        range: have,
        headers: options.headers,
        onResponse: open,
        sink: function (chunk, received, length) {
          // A fetch that names no status (a stand-in) sends the whole file.
          if (!out) open(200, length ? { 'content-length': String(length) } : {});
          out.write(chunk);
          if (options.onProgress) options.onProgress(base + received, expected || stated || 0);
        }
      }).then(function () {
        // A file of no bytes is a file all the same.
        if (!out) { try { open(200, {}); } catch (e) { return reject(e); } }
        close(function () { if (fault) reject(fault); else resolve({ stated: stated }); });
      }, function (e) {
        close(function () { reject(e); });
      });
    });
  };
  const from = await sizeOf();
  let resumed = 0;
  let fruitless = 0;
  for (;;) {
    let have = await sizeOf();
    // A part as long as the file or longer is the file, or not this file's.
    if (expected && have === expected) return { bytes: have, resumed: resumed, from: from };
    if (expected && have > expected) { await fsp.rm(part, { force: true }); have = 0; }
    let failure;
    try {
      const got = await once(have);
      const now = await sizeOf();
      const whole = expected || got.stated;
      if (!whole || now === whole) return { bytes: now, resumed: resumed, from: from };
      // The answer ended without an error and short of what it stated: as a break.
      failure = new CatalogError(now < whole ? 'short' : 'size', 'downloaded ' + now + ' bytes, expected ' + whole);
    } catch (e) {
      failure = e;
    }
    const code = failure && failure.code;
    // The part is not this file's: the server holds less than the part, or
    // answered another range, or states another size. It goes; a size that
    // is not the one expected is the caller's to hear of at once.
    if (code === 'http-416' || code === 'range' || code === 'size') await fsp.rm(part, { force: true });
    const broke = transient(failure) || code === 'timeout' || code === 'short' || code === 'http-416' || code === 'range';
    if (!broke) throw failure;
    const now = await sizeOf();
    fruitless = now - have >= MIN_STEP_BYTES ? 0 : fruitless + 1;
    if (fruitless > waits.length || resumed >= MAX_RESUMES) {
      // An answer that keeps ending whole and short of the size expected is
      // a file of another size, as it was called before downloads carried on.
      throw code === 'short' ? new CatalogError('size', failure.message) : failure;
    }
    const wait = waits[Math.max(0, fruitless - 1)];
    resumed += 1;
    if (options.told) options.told(failure, { at: now, total: expected, attempt: resumed + 1, wait: wait });
    await sleep(wait);
  }
}

// A file downloaded (carried on where a part of it is kept), held to its
// SHA-256 and then named `file`. A part kept from an earlier try whose
// whole does not match may have been of another file: it goes and the
// file is brought down once more from its first byte; a mismatch on a
// download begun here stands, and the part goes with it. `options` as
// `download`, with `sha256` the digest, `onVerify()` before the read and
// `dropped(bytes)` when a kept part was not of the file.
async function downloadChecked(url, file, options) {
  options = options || {};
  for (let again = 0; ; again++) {
    const got = await download(url, file, options);
    if (options.onVerify) options.onVerify();
    const digest = await sha256Of(file + '.part');
    if (digest === options.sha256) {
      await fsp.rename(file + '.part', file);
      return Object.assign(got, { sha256: digest });
    }
    await fsp.rm(file + '.part', { force: true });
    if (got.from > 0 && again === 0) {
      if (options.dropped) options.dropped(got.from);
      continue;
    }
    throw new CatalogError('checksum', path.basename(file) + ' is not the file its checksum names');
  }
}

// The parts of other downloads of the same kind beside this one: a part of
// `glass-0.11.40.zip` when `glass-0.11.41.zip` is wanted. They go, so that
// at most one part of a kind is ever kept.
async function forgetOtherParts(file) {
  const dir = path.dirname(file);
  const kind = function (name) { return name.replace(/\d+\.\d+\.\d+/, 'N'); };
  const mine = path.basename(file) + '.part';
  let names = [];
  try { names = await fsp.readdir(dir); } catch (e) { return; }
  for (const name of names) {
    if (name !== mine && /\.part$/.test(name) && kind(name) === kind(mine)) await fsp.rm(path.join(dir, name), { force: true });
  }
}

function validIndex(index) {
  return index && typeof index === 'object' && index.version === INDEX_VERSION &&
    typeof index.base === 'string' && Array.isArray(index.templates);
}

function validEntry(entry) {
  return entry && typeof entry === 'object' && NAME.test(String(entry.name)) &&
    typeof entry.zip === 'string' && /^[0-9a-f]{64}$/.test(String(entry.sha256)) &&
    Number.isInteger(entry.bytes) && entry.bytes > 0 && entry.bytes <= MAX_ZIP_BYTES &&
    Array.isArray(entry.units);
}

class Catalog {
  // dir: where the index, thumbnails, downloads and the installed record live.
  constructor(options) {
    this.dir = options.dir;
    this.logger = options.logger || console;
    this.indexUrl = options.indexUrl || INDEX_URL;
    this.retryWaits = options.retryWaits || RETRY_WAITS_MS;
    this.index = null;
    this.etag = null;
    this.fetchedAt = null;
    this.installed = {};
    this.thumbsDir = path.join(this.dir, 'thumbs');
    this.downloadsDir = path.join(this.dir, 'downloads');
  }

  async init() {
    await fsp.mkdir(this.thumbsDir, { recursive: true });
    await fsp.mkdir(this.downloadsDir, { recursive: true });
    try {
      const saved = JSON.parse(await fsp.readFile(path.join(this.dir, 'index.json'), 'utf8'));
      if (validIndex(saved.index)) {
        this.index = saved.index;
        this.etag = saved.etag || null;
        this.fetchedAt = saved.fetchedAt || null;
      }
    } catch (e) { /* no index yet */ }
    try {
      const installed = JSON.parse(await fsp.readFile(path.join(this.dir, 'installed.json'), 'utf8'));
      if (installed && typeof installed === 'object') this.installed = installed;
    } catch (e) { /* nothing installed from the catalog yet */ }
    // What an earlier run left in the downloads folder: the zip of an
    // interrupted install, a package kept for its Download link, and the
    // folders a cut, a package or an uploaded backup was staged in. Each is
    // removed by itself, a folder with what it holds, so that one that will
    // not go does not keep the rest.
    let left = [];
    try { left = await fsp.readdir(this.downloadsDir); } catch (e) { /* nothing to clear */ }
    for (const name of left) {
      try {
        await fsp.rm(path.join(this.downloadsDir, name), { recursive: true, force: true });
      } catch (e) {
        this.logger.warn('glass: manager catalog: ' + name + ' could not be cleared from the downloads: ' + e.message);
      }
    }
  }

  // Fetch the index when it changed; the cached one stays on 304 or failure.
  async refresh() {
    const headers = {};
    if (this.etag && this.index) headers['if-none-match'] = this.etag;
    const res = await get(this.indexUrl, { headers: headers, limit: MAX_INDEX_BYTES });
    if (res.status === 304) {
      this.fetchedAt = new Date().toISOString();
      await this.saveIndex();
      return { changed: false, index: this.index };
    }
    let parsed;
    try {
      parsed = JSON.parse(res.body.toString('utf8'));
    } catch (e) {
      throw new CatalogError('bad-index', 'the index is not JSON');
    }
    if (!validIndex(parsed)) throw new CatalogError('bad-index', 'the index has an unexpected shape');
    parsed.templates = parsed.templates.filter(validEntry);
    this.index = parsed;
    this.etag = res.headers.etag || null;
    this.fetchedAt = new Date().toISOString();
    await this.saveIndex();
    return { changed: true, index: this.index };
  }

  async saveIndex() {
    const file = path.join(this.dir, 'index.json');
    const tmp = file + '.tmp';
    await fsp.writeFile(tmp, JSON.stringify({ etag: this.etag, fetchedAt: this.fetchedAt, index: this.index }));
    await fsp.rename(tmp, file);
  }

  async saveInstalled() {
    const file = path.join(this.dir, 'installed.json');
    const tmp = file + '.tmp';
    await fsp.writeFile(tmp, JSON.stringify(this.installed, null, 1));
    await fsp.rename(tmp, file);
  }

  entries() {
    return this.index ? this.index.templates : [];
  }

  // Whether the index as kept is older than `ms`; true without one.
  stale(ms) {
    if (!this.index || !this.fetchedAt) return true;
    const at = Date.parse(this.fetchedAt);
    return !(at > 0) || Date.now() - at > ms;
  }

  find(name) {
    return this.entries().find(function (t) { return t.name === name; }) || null;
  }

  url(relative) {
    return new URL(relative, this.index.base).toString();
  }

  // The cached thumbnail of an entry, fetched once. Null without one.
  async thumb(name) {
    const entry = this.find(name);
    if (!entry || !entry.thumb) return null;
    const file = path.join(this.thumbsDir, name + path.extname(entry.thumb));
    try {
      await fsp.access(file);
      return file;
    } catch (e) { /* fetch it */ }
    const res = await get(this.url(entry.thumb), { limit: MAX_THUMB_BYTES });
    const tmp = file + '.part';
    await fsp.writeFile(tmp, res.body);
    await fsp.rename(tmp, file);
    return file;
  }

  // Download an entry's zip and hold it to the index's size and SHA-256.
  // Resolves with the file; a mismatch removes it and rejects. A
  // connection that breaks has the download carried on from where it stood.
  async download(entry, onProgress) {
    if (!validEntry(entry)) throw new CatalogError('bad-entry', 'the catalog entry is not usable');
    const self = this;
    const file = path.join(this.downloadsDir, entry.name + '.zip');
    try {
      const got = await downloadChecked(this.url(entry.zip), file, {
        sha256: entry.sha256,
        bytes: entry.bytes,
        limit: entry.bytes,
        waits: this.retryWaits,
        onProgress: onProgress,
        told: function (e, said) {
          self.logger.info('glass: catalog: the download of ' + entry.name + ' broke at ' + said.at + ' of ' + (said.total || '?') + ' bytes (' + e.message + '); carried on in ' + (said.wait / 1000) + ' s, attempt ' + said.attempt);
        },
        dropped: function (bytes) {
          self.logger.info('glass: catalog: the ' + bytes + ' bytes an earlier try had left of ' + entry.name + ' were not of this file: dropped, and the file brought down again');
        }
      });
      if (got.from > 0) self.logger.info('glass: catalog: ' + entry.name + ' carried on from the ' + got.from + ' bytes an earlier try had left');
      return file;
    } catch (e) {
      await fsp.rm(file, { force: true });
      if (e instanceof CatalogError && e.code === 'checksum') throw new CatalogError('checksum', 'the download does not match the catalog checksum');
      throw e;
    }
  }

  // Download an entry by name. When the file on the server no longer
  // matches the index as kept, the index is fetched again and the download
  // made once more against the entry as it stands now; the first error
  // stands when the index has not moved. Resolves with the file and the
  // entry it matches.
  async downloadCurrent(name, onProgress) {
    let entry = this.find(name);
    if (!entry) throw new CatalogError('unknown', 'the catalog does not list ' + name);
    try {
      return { file: await this.download(entry, onProgress), entry: entry };
    } catch (e) {
      if (!(e instanceof CatalogError) || ['too-large', 'size', 'checksum'].indexOf(e.code) === -1) throw e;
      const before = entry.sha256;
      await this.refresh();
      entry = this.find(name);
      if (!entry) throw new CatalogError('unknown', 'the catalog no longer lists ' + name);
      if (entry.sha256 === before) throw e;
      this.logger.info('glass: catalog: ' + name + ' changed since the index was fetched; downloading it again');
      return { file: await this.download(entry, onProgress), entry: entry };
    }
  }

  async discard(file) {
    await fsp.rm(file, { force: true });
  }

  async markInstalled(entry, folders) {
    this.installed[entry.name] = {
      sha256: entry.sha256,
      at: new Date().toISOString(),
      folders: folders
    };
    await this.saveInstalled();
  }

  // Forget catalog entries whose folders are all gone.
  async forgetFolder(install, folder) {
    let changed = false;
    for (const name of Object.keys(this.installed)) {
      const record = this.installed[name];
      const kept = (record.folders || []).filter(function (f) { return !(f.install === install && f.folder === folder); });
      if (kept.length !== (record.folders || []).length) {
        changed = true;
        if (kept.length === 0) delete this.installed[name];
        else record.folders = kept;
      }
    }
    if (changed) await this.saveInstalled();
  }

  // How an entry stands on this player: absent, installed, or an update.
  state(entry, exists) {
    const record = this.installed[entry.name];
    if (!record) {
      const anyPresent = (entry.units || []).some(function (u) { return exists(u.install, u.folder); });
      return anyPresent ? 'present' : 'absent';
    }
    const allPresent = (record.folders || []).every(function (f) { return exists(f.install, f.folder); });
    if (!allPresent) return 'absent';
    return record.sha256 === entry.sha256 ? 'installed' : 'update';
  }
}

module.exports = {
  Catalog: Catalog,
  CatalogError: CatalogError,
  get: get,
  download: download,
  downloadChecked: downloadChecked,
  forgetOtherParts: forgetOtherParts,
  sha256Of: sha256Of,
  transient: transient,
  validIndex: validIndex,
  validEntry: validEntry,
  INDEX_URL: INDEX_URL
};
