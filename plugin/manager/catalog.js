'use strict';
// The theme catalog: the index peppy_templates publishes, kept on the
// player with its ETag, the thumbnails it names fetched as they are looked
// at, and the zips downloaded and checked against the index's checksum
// before anything is unpacked. What was installed from the catalog is
// remembered by name and checksum, so an updated zip shows as an update.

const crypto = require('crypto');
const fs = require('fs');
const fsp = require('fs/promises');
const http = require('http');
const https = require('https');
const path = require('path');

const INDEX_URL = 'https://raw.githubusercontent.com/foonerd/peppy_templates/main/catalog/index.json';
const INDEX_VERSION = 1;
const FETCH_TIMEOUT_MS = 20000;
const DOWNLOAD_TIMEOUT_MS = 10 * 60 * 1000;
const MAX_REDIRECTS = 5;
const MAX_INDEX_BYTES = 16 * 1024 * 1024;
const MAX_THUMB_BYTES = 2 * 1024 * 1024;
const MAX_ZIP_BYTES = 512 * 1024 * 1024;
const NAME = /^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,127}$/;

class CatalogError extends Error {
  constructor(code, message) {
    super(message || code);
    this.code = code;
  }
}

// One HTTP GET with redirects and a timeout. `sink` takes each chunk; with
// no sink the body is collected up to `limit` bytes.
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
      const req = client.get(parsed, { headers: Object.assign({ 'user-agent': 'glass-manager' }, options.headers || {}) }, function (res) {
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
        if (status !== 200) {
          res.resume();
          return fail(new CatalogError('http-' + status, 'HTTP ' + status + ' for ' + target));
        }
        const length = parseInt(res.headers['content-length'], 10);
        if (options.limit && length > options.limit) {
          res.destroy();
          return fail(new CatalogError('too-large', 'response larger than allowed'));
        }
        const chunks = [];
        let received = 0;
        res.on('data', function (chunk) {
          received += chunk.length;
          if (options.limit && received > options.limit) {
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
          resolve({ status: 200, headers: res.headers, body: options.sink ? null : Buffer.concat(chunks), received: received });
        });
        res.on('error', fail);
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
    // Downloads left by an interrupted install.
    try {
      for (const name of await fsp.readdir(this.downloadsDir)) {
        await fsp.rm(path.join(this.downloadsDir, name), { force: true });
      }
    } catch (e) { /* nothing to clear */ }
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

  // Download an entry's zip and check its size and SHA-256 against the
  // index. Resolves with the file; a mismatch removes it and rejects.
  async download(entry, onProgress) {
    if (!validEntry(entry)) throw new CatalogError('bad-entry', 'the catalog entry is not usable');
    const file = path.join(this.downloadsDir, entry.name + '.zip');
    const tmp = file + '.part';
    const hash = crypto.createHash('sha256');
    const out = fs.createWriteStream(tmp);
    let received = 0;
    try {
      await new Promise(function (resolve, reject) {
        out.on('error', reject);
        get(this.url(entry.zip), {
          timeout: DOWNLOAD_TIMEOUT_MS,
          limit: entry.bytes,
          sink: function (chunk, total) {
            hash.update(chunk);
            received = total;
            if (!out.write(chunk)) { /* the file keeps up with the network on a player */ }
            if (onProgress) onProgress(received, entry.bytes);
          }
        }).then(function () {
          out.end(resolve);
        }, function (e) {
          out.destroy();
          reject(e);
        });
      }.bind(this));
      if (received !== entry.bytes) throw new CatalogError('size', 'downloaded ' + received + ' bytes, expected ' + entry.bytes);
      const digest = hash.digest('hex');
      if (digest !== entry.sha256) throw new CatalogError('checksum', 'the download does not match the catalog checksum');
      await fsp.rename(tmp, file);
      return file;
    } catch (e) {
      await fsp.rm(tmp, { force: true });
      await fsp.rm(file, { force: true });
      throw e;
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
  validIndex: validIndex,
  validEntry: validEntry,
  INDEX_URL: INDEX_URL
};
