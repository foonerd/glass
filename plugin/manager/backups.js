'use strict';
// A settings backup: a folder under the backups root with a manifest and
// the files a player's settings live in. Three the player always has,
// config.json (the settings page's and the Manager's own), the meter
// configuration kept as peppymeter_config.txt and the spectrum
// configuration as spectrum_config.txt, and one it has once a forecast
// place was chosen, weather.json (the place and the unit). A backup takes
// what is there; a restore puts back what the backup has and leaves alone
// what it lacks, so a backup made before weather.json rode along (schema
// 1) says nothing of the place and does not clear it. Every answer carries
// an error code that is a string of the plugin's, so the page can say it.

const fs = require('fs');
const path = require('path');
// The plugin has `ini` for the engine files; where the module runs without it (the tests) an engine file is held to its shape alone.
let ini = null;
try { ini = require('ini'); } catch (e) { ini = null; }

const SCHEMA_VERSION = 2;
const MANIFEST = 'manifest.json';
const NAME = /^[A-Za-z0-9 _.\-]{1,64}$/;

function json(text) { JSON.parse(text); }
function iniText(text) {
  if (ini) { ini.parse(text); return; }
  if (!/^\s*\[[^\]]+\]/m.test(String(text))) throw new Error('not an ini file');
}

// The files, in the order they are written: the key names the source or
// the target the caller gives, the name is the file's name in the backup.
const FILES = [
  { key: 'config', name: 'config.json', required: true, parse: json, corrupt: 'GLASS.BACKUP_CONFIG_CORRUPT' },
  { key: 'meter', name: 'peppymeter_config.txt', required: true, parse: iniText, corrupt: 'GLASS.BACKUP_PEPPYCONF_CORRUPT' },
  { key: 'spectrum', name: 'spectrum_config.txt', required: true, parse: iniText, corrupt: 'GLASS.BACKUP_SPECTRUMCONF_CORRUPT' },
  { key: 'weather', name: 'weather.json', required: false, parse: json, corrupt: 'GLASS.BACKUP_WEATHER_CORRUPT' }
];

function exists(file) { try { return fs.statSync(file).isFile(); } catch (e) { return false; } }

function copy(from, to) {
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
}

// A backup written under `dir` from `sources` ({ config, meter, spectrum,
// weather } as paths): the required ones must be there, the optional ones
// ride along when they are. `meta` names it (name, pluginVersion,
// automatic). Answers { ok, files } or { error, message }.
function create(dir, sources, meta) {
  meta = meta || {};
  for (const f of FILES) {
    if (f.required && !exists(sources[f.key] || '')) return { error: 'GLASS.BACKUP_SOURCE_MISSING', message: f.name };
  }
  fs.mkdirSync(dir, { recursive: true });
  const written = [];
  for (const f of FILES) {
    const from = sources[f.key];
    if (!from || !exists(from)) continue;
    copy(from, path.join(dir, f.name));
    written.push(f.name);
  }
  const manifest = {
    schema_version: SCHEMA_VERSION,
    plugin_version: meta.pluginVersion || '',
    created: new Date().toISOString(),
    name: meta.name || path.basename(dir),
    files: written,
    automatic: meta.automatic === true
  };
  fs.writeFileSync(path.join(dir, MANIFEST), JSON.stringify(manifest, null, 2));
  return { ok: true, files: written };
}

// The backup under `dir` read and held to its rules: a manifest with a
// schema this plugin knows, the required files there and parsable, the
// optional ones parsable where present. Answers { ok, manifest, files }
// (the names present) or { error }.
function check(dir) {
  const manifestPath = path.join(dir, MANIFEST);
  if (!exists(manifestPath)) return { error: 'GLASS.BACKUP_NOT_FOUND' };
  let manifest;
  try { manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_MANIFEST_INVALID' }; }
  if (!manifest || typeof manifest !== 'object' || manifest.schema_version === undefined) return { error: 'GLASS.BACKUP_MANIFEST_INVALID' };
  if (manifest.schema_version > SCHEMA_VERSION) return { error: 'GLASS.BACKUP_SCHEMA_UNSUPPORTED' };
  const present = [];
  for (const f of FILES) {
    const file = path.join(dir, f.name);
    if (!exists(file)) {
      if (f.required) return { error: 'GLASS.BACKUP_FILES_MISSING' };
      continue;
    }
    try { f.parse(fs.readFileSync(file, 'utf8')); } catch (e) { return { error: f.corrupt }; }
    present.push(f.name);
  }
  return { ok: true, manifest: manifest, files: present };
}

// The backup's files put back over `targets` ({ config, meter, spectrum,
// weather } as paths): each file the backup has goes to its target, each
// it lacks leaves the target as it stands. Checked first; answers
// { ok, restored, left } (keys) or { error }.
function restore(dir, targets) {
  const held = check(dir);
  if (held.error) return held;
  const restored = [];
  const left = [];
  for (const f of FILES) {
    const to = targets[f.key];
    if (!to) continue;
    if (held.files.indexOf(f.name) === -1) { left.push(f.key); continue; }
    copy(path.join(dir, f.name), to);
    restored.push(f.key);
  }
  return { ok: true, restored: restored, left: left, manifest: held.manifest };
}

// A backup unpacked elsewhere taken into the backups root under its own
// name, or the one asked for, with a number when the name is taken; the
// files it has come along, the manifest says when it was taken in.
function adopt(stagingDir, backupsDir, wantedName) {
  const held = check(stagingDir);
  if (held.error) return held.error === 'GLASS.BACKUP_NOT_FOUND' ? { error: 'GLASS.BACKUP_MANIFEST_INVALID' } : held;
  let base = String(wantedName || held.manifest.name || 'imported').trim().replace(/[^A-Za-z0-9 _.\-]/g, '-').replace(/^[.\-]+/, '').slice(0, 56).trim() || 'imported';
  if (!NAME.test(base) || base.indexOf('..') !== -1) base = 'imported';
  fs.mkdirSync(backupsDir, { recursive: true });
  let name = base;
  let n = 1;
  while (fs.existsSync(path.join(backupsDir, name))) name = base + '-' + (++n);
  const target = path.join(backupsDir, name);
  fs.mkdirSync(target);
  held.files.forEach(function (file) { copy(path.join(stagingDir, file), path.join(target, file)); });
  const manifest = Object.assign({}, held.manifest, { name: name, imported: new Date().toISOString(), files: held.files });
  fs.writeFileSync(path.join(target, MANIFEST), JSON.stringify(manifest, null, 2));
  return { ok: true, name: name, files: held.files };
}

module.exports = { create, check, restore, adopt, FILES, SCHEMA_VERSION, MANIFEST, NAME };
