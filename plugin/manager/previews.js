'use strict';
// Previews of installed themes, drawn by the display itself: the glass
// binary renders every meter of a theme headless and writes a full-size
// picture and a thumbnail of each. Renders run one at a time and are
// redone when the theme's meters file changes or the plugin's version does.

const fsp = require('fs/promises');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const THUMB_WIDTH = 320;
const SETTLE_SECONDS = 0.5;
const RENDER_TIMEOUT_MS = 180000;
const STAMP = '.stamp.json';

class Previews {
  // dir: where the previews live; launcher: the run script; env(): the
  // environment the display is launched with; version: what stamps a render.
  constructor(options) {
    this.dir = options.dir;
    this.launcher = options.launcher;
    this.env = options.env;
    this.version = options.version || '';
    this.logger = options.logger || console;
    this.uid = options.uid;
    this.gid = options.gid;
    this.queue = [];
    this.running = null;
    this.pending = new Map();
  }

  async init() {
    await fsp.mkdir(this.dir, { recursive: true });
    // Renders interrupted by a restart.
    try {
      for (const name of await fsp.readdir(this.dir)) {
        if (name.startsWith('.render-')) await fsp.rm(path.join(this.dir, name), { recursive: true, force: true });
      }
    } catch (e) { /* nothing to clear */ }
  }

  themeDir(theme) {
    return path.join(this.dir, theme);
  }

  // fresh, stale, none, or rendering.
  async status(theme, metersFile) {
    if (this.running && this.running.theme === theme) return 'rendering';
    if (this.pending.has(theme)) return 'queued';
    let stamp;
    try {
      stamp = JSON.parse(await fsp.readFile(path.join(this.themeDir(theme), STAMP), 'utf8'));
    } catch (e) {
      return 'none';
    }
    let mtime = 0;
    try {
      mtime = (await fsp.stat(metersFile)).mtimeMs;
    } catch (e) {
      return 'none';
    }
    return (stamp.metersMtime === mtime && stamp.version === this.version) ? 'fresh' : 'stale';
  }

  // The rendered meters of a theme, in the order the render wrote them.
  async list(theme) {
    let stamp;
    try {
      stamp = JSON.parse(await fsp.readFile(path.join(this.themeDir(theme), STAMP), 'utf8'));
    } catch (e) {
      return [];
    }
    return stamp.meters || [];
  }

  file(theme, meter, thumb) {
    const stem = String(meter).replace(/\//g, '_');
    return path.join(this.themeDir(theme), stem + (thumb ? '.thumb.png' : '.png'));
  }

  // Render a theme unless a fresh render exists. Resolves when done.
  ensure(theme, metersFile, force) {
    const self = this;
    if (self.pending.has(theme)) return self.pending.get(theme);
    const job = (async function () {
      if (!force) {
        const state = await self.status(theme, metersFile);
        if (state === 'fresh') return { rendered: false };
      }
      await self.slot(theme);
      try {
        return await self.render(theme, metersFile);
      } finally {
        self.release();
      }
    })();
    self.pending.set(theme, job);
    job.then(function () { self.pending.delete(theme); }, function () { self.pending.delete(theme); });
    return job;
  }

  // One render at a time.
  slot(theme) {
    const self = this;
    return new Promise(function (resolve) {
      self.queue.push({ theme: theme, go: resolve });
      self.next();
    });
  }

  next() {
    if (this.running || this.queue.length === 0) return;
    this.running = this.queue.shift();
    this.running.go();
  }

  release() {
    this.running = null;
    this.next();
  }

  async render(theme, metersFile) {
    const self = this;
    const work = path.join(self.dir, '.render-' + theme);
    await fsp.rm(work, { recursive: true, force: true });
    await fsp.mkdir(work, { recursive: true });
    // The display runs as the player's user; the plugin may not.
    if (self.uid !== undefined) {
      try { await fsp.chown(work, self.uid, self.gid); } catch (e) { /* same user, or not permitted: the render says */ }
    }
    const args = ['--headless', '--snapshot', work, '--thumb', String(THUMB_WIDTH), '--theme', theme, '--settle', String(SETTLE_SECONDS)];
    const env = Object.assign({}, self.env());
    delete env.DISPLAY;
    const started = Date.now();
    const output = await new Promise(function (resolve, reject) {
      const options = { env: env, stdio: ['ignore', 'pipe', 'pipe'] };
      if (self.uid !== undefined) { options.uid = self.uid; options.gid = self.gid; }
      const child = spawn(self.launcher, args, options);
      let out = '';
      let err = '';
      child.stdout.on('data', function (d) { out += d; if (out.length > 65536) out = out.slice(-32768); });
      child.stderr.on('data', function (d) { err += d; if (err.length > 65536) err = err.slice(-32768); });
      const timer = setTimeout(function () {
        child.kill('SIGKILL');
      }, RENDER_TIMEOUT_MS);
      child.on('error', function (e) { clearTimeout(timer); reject(e); });
      child.on('close', function (code, signal) {
        clearTimeout(timer);
        if (signal) return reject(new Error('the render was stopped (' + signal + ')'));
        if (code !== 0) return reject(new Error('the display left with code ' + code + (err ? ': ' + err.trim().split('\n').pop() : '')));
        resolve(out);
      });
    });
    const meters = [];
    output.split('\n').forEach(function (line) {
      const m = /^glass: snapshot (.+\.png)$/.exec(line);
      if (m && !m[1].endsWith('.thumb.png')) {
        const stem = path.basename(m[1], '.png');
        meters.push(stem);
      }
    });
    const produced = path.join(work, theme);
    if (meters.length === 0 || !fs.existsSync(produced)) {
      await fsp.rm(work, { recursive: true, force: true });
      throw new Error('the render produced no pictures');
    }
    let mtime = 0;
    try { mtime = (await fsp.stat(metersFile)).mtimeMs; } catch (e) { /* stamped as none */ }
    await fsp.writeFile(path.join(produced, STAMP), JSON.stringify({
      version: self.version,
      metersMtime: mtime,
      at: new Date().toISOString(),
      ms: Date.now() - started,
      meters: meters
    }));
    const target = self.themeDir(theme);
    await fsp.rm(target, { recursive: true, force: true });
    await fsp.rename(produced, target);
    await fsp.rm(work, { recursive: true, force: true });
    self.logger.info('glass: manager rendered ' + meters.length + ' previews of ' + theme + ' in ' + (Date.now() - started) + ' ms');
    return { rendered: true, meters: meters };
  }

  async drop(theme) {
    await fsp.rm(this.themeDir(theme), { recursive: true, force: true });
  }
}

module.exports = { Previews: Previews, THUMB_WIDTH: THUMB_WIDTH };
