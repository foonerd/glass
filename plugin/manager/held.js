'use strict';
// A release zip brought by hand that cannot go in alone, kept on the
// player for the other zip of the pair: a Glass that needs a newer
// glass-evo than the one installed, or a glass-evo that needs a later
// Glass. One zip of each kind at most, the newer replacing the older as
// it lands; each with what it says it is, so the pairing rule can read it
// without opening the zip again. The files and their note live in a
// folder of their own under the plugin's data, and survive a restart of
// the backend; the user discards one on the System tab, and the switch
// for install from a file going off discards both.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');

const NOTE = 'held.json';
const KINDS = ['glass', 'evo'];

class Held {
  constructor(options) {
    this.dir = options.dir;
    this.logger = options.logger;
    this.entries = {};
  }

  // The note read, and an entry whose file is gone forgotten.
  async init() {
    await fsp.mkdir(this.dir, { recursive: true });
    let saved = {};
    try {
      saved = JSON.parse(await fsp.readFile(path.join(this.dir, NOTE), 'utf8'));
    } catch (e) {
      saved = {};
    }
    this.entries = {};
    for (const kind of KINDS) {
      const e = saved && saved[kind];
      if (!e || typeof e !== 'object' || !e.file || !e.version) continue;
      if (!fs.existsSync(e.file)) continue;
      this.entries[kind] = e;
    }
    await this.save();
  }

  async save() {
    const file = path.join(this.dir, NOTE);
    await fsp.writeFile(file + '.tmp', JSON.stringify(this.entries));
    await fsp.rename(file + '.tmp', file);
  }

  // The entry of a kind, or null: `kind`, `file`, `version`, and `least`
  // (a Glass) or `requires` (a glass-evo), `verified` and `at`.
  get(kind) {
    return this.entries[kind] || null;
  }

  // Keep a verified zip as the one of its kind, the one before it gone.
  // `facts` carries version and least or requires, and how it was verified.
  async keep(kind, file, facts) {
    if (KINDS.indexOf(kind) === -1) throw new Error('not a kind of zip: ' + kind);
    const name = (kind === 'glass' ? 'glass-' : 'glass-evo-') + facts.version + '.zip';
    const to = path.join(this.dir, name);
    await this.drop(kind);
    if (path.resolve(file) !== path.resolve(to)) {
      await fsp.rm(to, { force: true });
      try {
        await fsp.rename(file, to);
      } catch (e) {
        await fsp.copyFile(file, to);
        await fsp.rm(file, { force: true });
      }
    }
    const entry = { kind: kind, file: to, version: facts.version, verified: facts.verified || null, at: new Date().toISOString() };
    if (kind === 'glass') entry.least = facts.least || null;
    else entry.requires = facts.requires || null;
    this.entries[kind] = entry;
    await this.save();
    if (this.logger) this.logger.info('glass: manager: ' + name + ' kept for the other zip of the pair');
    return entry;
  }

  // The entry of a kind forgotten and its file removed; answers it, or null.
  async drop(kind) {
    const e = this.entries[kind] || null;
    if (!e) return null;
    delete this.entries[kind];
    await fsp.rm(e.file, { force: true });
    await this.save();
    if (this.logger) this.logger.info('glass: manager: ' + path.basename(e.file) + ' no longer kept');
    return e;
  }

  // The entry taken out to be installed: forgotten here, the file left
  // for the caller, who removes it when done.
  async take(kind) {
    const e = this.entries[kind] || null;
    if (!e) return null;
    delete this.entries[kind];
    await this.save();
    return e;
  }

  async clear() {
    for (const kind of KINDS) await this.drop(kind);
  }

  // What is kept, for the page, the sheet and the checks: each kind's
  // version and the version of the other it waits for.
  summary() {
    const out = { glass: null, evo: null };
    const g = this.entries.glass;
    const e = this.entries.evo;
    if (g) out.glass = { version: g.version, needs: g.least || null, at: g.at || null };
    if (e) out.evo = { version: e.version, needs: e.requires || null, at: e.at || null };
    return out;
  }
}

module.exports = { Held, KINDS };
