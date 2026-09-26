'use strict';
// The Glass Manager: a small web application the plugin serves on its own
// port, for everything that manages the display rather than operates it.
// Installed themes with previews drawn by the display, the catalog and its
// installs, uploads, the meter rotation, artwork, backups and a status page.
//
// The server knows the plugin through a narrow interface (see index.js,
// "The manager's view of the plugin") and the file trees through the paths
// that interface names. It holds no Volumio state of its own.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const os = require('os');
const express = require('express');

const { Zip, ZipError, unitsOf, extractUnit, safeFolderName } = require('./zip');
const { Catalog, CatalogError } = require('./catalog');
const { Previews } = require('./previews');

const DEFAULT_PORT = 5582;
const MAX_UPLOAD_BYTES = 512 * 1024 * 1024;
const JOBS_KEPT = 50;
const METER_NAME = /^[^/\\\0]{1,128}$/;
const SIZE_PREFIX = /^(\d{3,4})x(\d{3,4})/;

class Manager {
  constructor(plugin) {
    this.plugin = plugin;
    this.logger = plugin.logger;
    this.app = null;
    this.server = null;
    this.port = DEFAULT_PORT;
    this.jobs = [];
    this.jobSeq = 0;
    this.lock = Promise.resolve();
    const paths = plugin.managerPaths();
    this.paths = paths;
    this.catalog = new Catalog({ dir: path.join(paths.dataDir, 'catalog'), logger: this.logger });
    this.previews = new Previews({
      dir: path.join(paths.dataDir, 'previews'),
      launcher: paths.launcher,
      env: function () { return plugin.launchEnv(); },
      version: paths.version,
      logger: this.logger,
      uid: 1000,
      gid: 1000
    });
  }

  // The port the manager listens on, and where it is reached from a browser.
  url() {
    const host = this.plugin.managerHost();
    return 'http://' + host + ':' + this.port + '/';
  }

  async start(port) {
    const self = this;
    self.port = parseInt(port, 10) || DEFAULT_PORT;
    await self.catalog.init();
    await self.previews.init();
    const app = express();
    app.disable('x-powered-by');
    app.use(express.json({ limit: '1mb' }));
    app.use(function (req, res, next) {
      res.set('Cache-Control', 'no-cache');
      next();
    });
    self.routes(app);
    app.use(function (err, req, res, next) { // eslint-disable-line no-unused-vars
      self.logger.error('glass: manager ' + req.method + ' ' + req.path + ': ' + (err && err.message ? err.message : err));
      if (res.headersSent) return;
      res.status(err && err.status ? err.status : 500).json(failure(err));
    });
    self.app = app;
    await new Promise(function (resolve, reject) {
      const server = app.listen(self.port, function () {
        self.server = server;
        resolve();
      });
      server.on('error', reject);
    });
    self.logger.info('glass: manager listening on port ' + self.port);
  }

  async stop() {
    const server = this.server;
    this.server = null;
    if (!server) return;
    await new Promise(function (resolve) {
      server.close(function () { resolve(); });
      // Keep-alive connections would hold the port; the plugin is stopping.
      if (typeof server.closeAllConnections === 'function') server.closeAllConnections();
    });
    this.logger.info('glass: manager stopped');
  }

  // Work on the theme trees runs one job at a time.
  exclusive(fn) {
    const run = this.lock.then(fn, fn);
    this.lock = run.then(function () {}, function () {});
    return run;
  }

  // ---- routes ----------------------------------------------------------

  routes(app) {
    const self = this;
    const page = path.join(__dirname, 'manage.html');
    const wrap = function (fn) {
      return function (req, res, next) {
        Promise.resolve().then(function () { return fn(req, res); }).catch(next);
      };
    };

    app.get(['/', '/manage'], function (req, res) {
      res.sendFile(page);
    });

    app.get('/api/i18n', function (req, res) {
      const lang = String(req.query.lang || self.plugin.managerLanguage() || 'en').replace(/[^a-z]/gi, '').slice(0, 5) || 'en';
      res.json({ lang: lang, strings: self.plugin.managerStrings(lang) });
    });

    app.get('/api/status', wrap(async function (req, res) {
      res.json(await self.status());
    }));

    // Installed themes.
    app.get('/api/themes', wrap(async function (req, res) {
      res.json(await self.themes());
    }));

    app.post('/api/themes/:folder/activate', wrap(async function (req, res) {
      const folder = self.folderParam(req);
      const result = await self.exclusive(function () { return self.plugin.activateTheme(folder); });
      if (result.error) return res.status(400).json({ error: result.error });
      res.json({ ok: true, active: folder, changed: result.changed !== false });
    }));

    app.delete('/api/themes/:folder', wrap(async function (req, res) {
      const folder = self.folderParam(req);
      const result = await self.exclusive(async function () {
        const outcome = self.plugin.removeTheme(folder);
        if (!outcome.error) {
          await self.previews.drop(folder);
          await self.catalog.forgetFolder('templates', folder);
          await self.catalog.forgetFolder('templates_spectrum', folder);
        }
        return outcome;
      });
      if (result.error) return res.status(400).json({ error: result.error });
      res.json({ ok: true, switchedTo: result.switchedTo || null });
    }));

    app.get('/api/themes/:folder/previews', wrap(async function (req, res) {
      const folder = self.folderParam(req);
      res.json(await self.previewsOf(folder));
    }));

    app.post('/api/themes/:folder/render', wrap(async function (req, res) {
      const folder = self.folderParam(req);
      const metersFile = path.join(self.paths.meterBase, folder, 'meters.txt');
      if (!fs.existsSync(metersFile)) return res.status(404).json({ error: 'not-found' });
      const force = !!(req.body && req.body.force);
      const wait = !!(req.body && req.body.wait);
      const job = self.previews.ensure(folder, metersFile, force);
      job.catch(function (e) {
        self.logger.warn('glass: manager preview of ' + folder + ': ' + e.message);
      });
      if (wait) {
        try {
          await job;
        } catch (e) {
          return res.status(500).json({ error: 'render-failed', message: e.message });
        }
      }
      res.json(await self.previewsOf(folder));
    }));

    app.get('/api/themes/:folder/preview/:meter', wrap(async function (req, res) {
      const folder = self.folderParam(req);
      const meter = String(req.params.meter || '');
      if (!METER_NAME.test(meter)) return res.status(400).json({ error: 'bad-meter' });
      const file = self.previews.file(folder, meter, req.query.thumb === '1');
      if (!fs.existsSync(file)) return res.status(404).json({ error: 'not-found' });
      res.type('png');
      res.sendFile(file, { maxAge: 0 });
    }));

    // The meter rotation of the active theme.
    app.get('/api/meter', function (req, res) {
      res.json(self.plugin.meterSelection());
    });

    app.post('/api/meter', wrap(async function (req, res) {
      const result = await self.exclusive(function () { return self.plugin.setMeterSelection(req.body || {}); });
      if (result.error) return res.status(400).json(result);
      res.json(Object.assign({ ok: true }, self.plugin.meterSelection()));
    }));

    // The catalog.
    app.get('/api/catalog', wrap(async function (req, res) {
      if (!self.catalog.index) {
        try {
          await self.catalog.refresh();
        } catch (e) {
          return res.json(self.catalogView(e));
        }
      }
      res.json(self.catalogView(null));
    }));

    app.post('/api/catalog/refresh', wrap(async function (req, res) {
      try {
        await self.catalog.refresh();
      } catch (e) {
        return res.status(502).json(self.catalogView(e));
      }
      res.json(self.catalogView(null));
    }));

    app.get('/api/catalog/thumb/:name', wrap(async function (req, res) {
      const name = String(req.params.name || '');
      if (!safeFolderName(name)) return res.status(400).json({ error: 'bad-name' });
      let file;
      try {
        file = await self.catalog.thumb(name);
      } catch (e) {
        return res.status(502).json(failure(e));
      }
      if (!file) return res.status(404).json({ error: 'not-found' });
      res.sendFile(file, { maxAge: 86400000 });
    }));

    app.post('/api/catalog/install', wrap(async function (req, res) {
      const name = String((req.body && req.body.name) || '');
      const entry = self.catalog.find(name);
      if (!entry) return res.status(404).json({ error: 'not-found' });
      const job = self.newJob('install', name);
      self.runInstall(job, entry);
      res.status(202).json({ ok: true, job: job });
    }));

    // A zip sent as the request body.
    app.post('/api/upload', wrap(async function (req, res) {
      const rawName = String(req.query.name || req.headers['x-file-name'] || 'upload.zip');
      const name = path.basename(rawName, path.extname(rawName)).replace(/[^A-Za-z0-9 ._()+-]/g, '_').slice(0, 96) || 'upload';
      if (!safeFolderName(name)) return res.status(400).json({ error: 'bad-name' });
      const file = path.join(self.catalog.downloadsDir, 'upload-' + Date.now() + '.zip');
      try {
        await self.receive(req, file, MAX_UPLOAD_BYTES);
      } catch (e) {
        await fsp.rm(file, { force: true });
        return res.status(e.code === 'too-large' ? 413 : 400).json(failure(e));
      }
      const job = self.newJob('upload', name);
      self.runUpload(job, file, name);
      res.status(202).json({ ok: true, job: job });
    }));

    app.get('/api/jobs', function (req, res) {
      res.json({ jobs: self.jobs.slice().reverse() });
    });

    app.get('/api/jobs/:id', function (req, res) {
      const job = self.jobs.find(function (j) { return String(j.id) === String(req.params.id); });
      if (!job) return res.status(404).json({ error: 'not-found' });
      res.json({ job: job });
    });

    // Artwork.
    app.get('/api/artwork', function (req, res) {
      res.json(self.plugin.artworkSettings());
    });

    app.post('/api/artwork', wrap(async function (req, res) {
      const result = self.plugin.setArtworkSettings(req.body || {});
      if (result.error) return res.status(400).json(result);
      res.json(Object.assign({ ok: true }, self.plugin.artworkSettings()));
    }));

    app.post('/api/artwork/clear-cache', wrap(async function (req, res) {
      const result = self.plugin.clearFanartImages();
      if (result.error) return res.status(500).json(result);
      res.json({ ok: true });
    }));

    // Settings the manager owns: the tag rules and whether themes survive an uninstall.
    app.get('/api/settings', function (req, res) {
      res.json(self.plugin.managerSettings());
    });

    app.post('/api/settings', wrap(async function (req, res) {
      const result = self.plugin.setManagerSettings(req.body || {});
      if (result.error) return res.status(400).json(result);
      res.json(Object.assign({ ok: true }, self.plugin.managerSettings()));
    }));

    // Backups of the settings.
    app.get('/api/backups', function (req, res) {
      res.json({ backups: self.plugin.backupList() });
    });

    app.post('/api/backups', wrap(async function (req, res) {
      const result = await self.exclusive(function () { return self.plugin.backupCreate(req.body && req.body.name); });
      if (result.error) return res.status(400).json(result);
      res.json({ ok: true, backups: self.plugin.backupList() });
    }));

    app.post('/api/backups/:name/restore', wrap(async function (req, res) {
      const result = await self.exclusive(function () { return self.plugin.backupRestore(req.params.name); });
      if (result.error) return res.status(400).json(result);
      res.json({ ok: true });
    }));

    app.delete('/api/backups/:name', wrap(async function (req, res) {
      const result = await self.exclusive(function () { return self.plugin.backupDelete(req.params.name); });
      if (result.error) return res.status(400).json(result);
      res.json({ ok: true, backups: self.plugin.backupList() });
    }));

    app.use('/api', function (req, res) {
      res.status(404).json({ error: 'not-found' });
    });
  }

  folderParam(req) {
    const folder = String(req.params.folder || '');
    if (!safeFolderName(folder)) {
      const e = new Error('bad folder name');
      e.status = 400;
      e.code = 'bad-folder';
      throw e;
    }
    return folder;
  }

  // ---- views -----------------------------------------------------------

  async status() {
    const plugin = this.plugin;
    const info = plugin.statusInfo();
    let free = null;
    try {
      const stat = await fsp.statfs(this.paths.dataDir);
      free = stat.bavail * stat.bsize;
    } catch (e) { /* unknown */ }
    let rings = [];
    try {
      rings = (await fsp.readdir('/dev/shm')).filter(function (n) { return n.startsWith('glasstap.'); });
    } catch (e) { /* none */ }
    return Object.assign(info, {
      manager: { port: this.port, url: this.url(), uptimeS: Math.round(process.uptime()) },
      catalog: {
        fetchedAt: this.catalog.fetchedAt,
        updated: this.catalog.index ? this.catalog.index.updated : null,
        count: this.catalog.entries().length,
        installed: Object.keys(this.catalog.installed).length
      },
      rings: rings,
      diskFree: free,
      hostname: os.hostname(),
      jobs: this.jobs.filter(function (j) { return j.state !== 'done' && j.state !== 'failed'; }).length
    });
  }

  async themes() {
    const self = this;
    const list = self.plugin.themeList();
    const byFolder = {};
    Object.keys(self.catalog.installed).forEach(function (name) {
      (self.catalog.installed[name].folders || []).forEach(function (f) {
        if (f.install === 'templates') byFolder[f.folder] = name;
      });
    });
    for (const theme of list) {
      const metersFile = path.join(self.paths.meterBase, theme.folder, 'meters.txt');
      theme.previews = theme.meters.length ? await self.previews.status(theme.folder, metersFile) : 'none';
      theme.catalog = byFolder[theme.folder] || null;
      const size = SIZE_PREFIX.exec(theme.folder);
      theme.width = size ? parseInt(size[1], 10) : theme.width || null;
      theme.height = size ? parseInt(size[2], 10) : theme.height || null;
    }
    return {
      active: self.plugin.activeTheme(),
      meterBase: self.paths.meterBase,
      spectrumBase: self.paths.spectrumBase,
      themes: list
    };
  }

  async previewsOf(folder) {
    const metersFile = path.join(this.paths.meterBase, folder, 'meters.txt');
    const state = await this.previews.status(folder, metersFile);
    const meters = await this.previews.list(folder);
    return {
      folder: folder,
      state: state,
      meters: meters.map(function (m) {
        const base = '/api/themes/' + encodeURIComponent(folder) + '/preview/' + encodeURIComponent(m);
        return { meter: m, url: base, thumb: base + '?thumb=1' };
      })
    };
  }

  catalogView(error) {
    const self = this;
    const exists = function (install, folder) {
      const base = install === 'templates' ? self.paths.meterBase : self.paths.spectrumBase;
      return fs.existsSync(path.join(base, folder));
    };
    const entries = self.catalog.entries().map(function (t) {
      return {
        name: t.name,
        kind: t.kind,
        category: t.category,
        width: t.width,
        height: t.height,
        bytes: t.bytes,
        units: t.units,
        thumb: t.thumb ? '/api/catalog/thumb/' + encodeURIComponent(t.name) : null,
        preview: t.preview ? self.catalog.url(t.preview) : null,
        state: self.catalog.state(t, exists)
      };
    });
    return {
      fetchedAt: self.catalog.fetchedAt,
      updated: self.catalog.index ? self.catalog.index.updated : null,
      source: self.catalog.indexUrl,
      error: error ? failure(error) : null,
      entries: entries
    };
  }

  // ---- jobs ------------------------------------------------------------

  newJob(kind, name) {
    const job = {
      id: ++this.jobSeq,
      kind: kind,
      name: name,
      state: 'queued',
      progress: { done: 0, total: 0 },
      folders: [],
      error: null,
      startedAt: new Date().toISOString(),
      endedAt: null
    };
    this.jobs.push(job);
    while (this.jobs.length > JOBS_KEPT) this.jobs.shift();
    return job;
  }

  finish(job, error) {
    job.endedAt = new Date().toISOString();
    if (error) {
      job.state = 'failed';
      job.error = failure(error);
      this.logger.warn('glass: manager ' + job.kind + ' ' + job.name + ' failed: ' + job.error.message);
    } else {
      job.state = 'done';
      this.logger.info('glass: manager ' + job.kind + ' ' + job.name + ' done: ' + job.folders.map(function (f) { return f.install + '/' + f.folder; }).join(', '));
    }
  }

  runInstall(job, entry) {
    const self = this;
    self.exclusive(async function () {
      let file = null;
      try {
        job.state = 'downloading';
        file = await self.catalog.download(entry, function (done, total) {
          job.progress = { done: done, total: total };
        });
        job.state = 'checking';
        const zip = await Zip.open(file);
        try {
          const found = await unitsOf(zip, entry.name);
          if (found.problems.length) throw new ZipError('bad-theme', found.problems.join('; '));
          if (!sameUnits(found.units, entry.units)) throw new ZipError('units-differ', 'the zip does not hold the folders the catalog names');
          job.state = 'unpacking';
          job.folders = await self.unpack(zip, found.units, job);
        } finally {
          await zip.close();
        }
        await self.catalog.markInstalled(entry, job.folders);
        await self.afterInstall(job);
        self.finish(job, null);
      } catch (e) {
        self.finish(job, e);
      } finally {
        if (file) await self.catalog.discard(file);
      }
    });
  }

  runUpload(job, file, name) {
    const self = this;
    self.exclusive(async function () {
      try {
        job.state = 'checking';
        const zip = await Zip.open(file);
        try {
          const found = await unitsOf(zip, name);
          if (found.units.length === 0) throw new ZipError('bad-theme', found.problems.join('; '));
          job.state = 'unpacking';
          job.folders = await self.unpack(zip, found.units, job);
        } finally {
          await zip.close();
        }
        for (const f of job.folders) {
          await self.catalog.forgetFolder(f.install, f.folder);
        }
        await self.afterInstall(job);
        self.finish(job, null);
      } catch (e) {
        self.finish(job, e);
      } finally {
        await fsp.rm(file, { force: true });
      }
    });
  }

  async unpack(zip, units, job) {
    const folders = [];
    for (const unit of units) {
      const root = unit.install === 'templates' ? this.paths.meterBase : this.paths.spectrumBase;
      await fsp.mkdir(root, { recursive: true });
      const result = await extractUnit(zip, unit, root, {
        onProgress: function (done, total) { job.progress = { done: done, total: total }; }
      });
      // The themes belong to the player's user, whoever unpacked them.
      await ownTree(path.join(root, result.folder), this.previews.uid, this.previews.gid);
      folders.push({ install: unit.install, folder: result.folder, kind: unit.kind, files: result.files, bytes: result.bytes, names: unit.names });
    }
    return folders;
  }

  // After folders landed: the plugin re-reads its lists, and the meter
  // folders get their previews drawn in the background.
  async afterInstall(job) {
    const self = this;
    try { self.plugin.afterThemesChanged(job.folders); } catch (e) {
      self.logger.warn('glass: manager after install: ' + e.message);
    }
    job.state = 'rendering';
    for (const f of job.folders) {
      if (f.install !== 'templates') continue;
      const metersFile = path.join(self.paths.meterBase, f.folder, 'meters.txt');
      self.previews.ensure(f.folder, metersFile, true).catch(function (e) {
        self.logger.warn('glass: manager preview of ' + f.folder + ': ' + e.message);
      });
    }
  }

  // The request body into a file, capped.
  receive(req, file, limit) {
    return new Promise(function (resolve, reject) {
      const out = fs.createWriteStream(file);
      let received = 0;
      let failed = false;
      const fail = function (code, message) {
        if (failed) return;
        failed = true;
        const e = new Error(message);
        e.code = code;
        out.destroy();
        req.destroy();
        reject(e);
      };
      req.on('data', function (chunk) {
        received += chunk.length;
        if (received > limit) return fail('too-large', 'the upload is larger than allowed');
      });
      req.on('error', function (e) { fail('network', e.message); });
      out.on('error', function (e) { fail('write', e.message); });
      out.on('finish', function () {
        if (failed) return;
        if (received === 0) return fail('empty', 'the upload is empty');
        resolve(received);
      });
      req.pipe(out);
    });
  }
}

// Give a tree to a user, when this process may. Nothing to do otherwise.
async function ownTree(dir, uid, gid) {
  if (uid === undefined || typeof process.getuid !== 'function' || process.getuid() !== 0) return;
  const walk = async function (p) {
    await fsp.chown(p, uid, gid);
    const stat = await fsp.lstat(p);
    if (!stat.isDirectory()) return;
    for (const name of await fsp.readdir(p)) await walk(path.join(p, name));
  };
  try {
    await walk(dir);
  } catch (e) { /* the files are still readable by everyone */ }
}

function sameUnits(found, expected) {
  if (!Array.isArray(expected) || found.length !== expected.length) return false;
  const key = function (u) { return u.install + '|' + u.folder + '|' + u.from; };
  const a = found.map(key).sort();
  const b = expected.map(key).sort();
  return a.every(function (k, i) { return k === b[i]; });
}

function failure(e) {
  if (!e) return { error: 'unknown', message: '' };
  const code = (e instanceof ZipError || e instanceof CatalogError || e.code) ? String(e.code) : 'error';
  return { error: code, message: String(e.message || e) };
}

module.exports = { Manager: Manager, DEFAULT_PORT: DEFAULT_PORT, sameUnits: sameUnits };
