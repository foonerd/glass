'use strict';
// The engine's two configuration files, meter.txt and spectrum.txt, and the
// one rule for them: a release's files live where that release reads them,
// and nothing but that release's own start makes or moves them.
//
// From 0.9.42 a release reads them under the player's data folder, out of
// the plugin folder that Volumio replaces whole at every install; before
// 0.9.42 a release reads them in the plugin folder. `place` names the
// folder by version, for every tool that writes the files for a release
// other than the one running: the Manager's restore after an install, the
// way back to the stable release, the installer's way back.

const fs = require('fs');
const path = require('path');

const NAMES = ['meter', 'spectrum'];
// The first release that reads the files under the data folder.
const MOVED_AT = '0.9.42';

function parts(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)/.exec(String(version || ''));
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

// -1, 0 or 1 for a before b; null when either is not a version.
function compare(a, b) {
  const x = parts(a);
  const y = parts(b);
  if (!x || !y) return null;
  for (let i = 0; i < 3; i++) {
    if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  }
  return 0;
}

// Where a release of `version` reads the files: `<dataDir>/config` from
// 0.9.42 on, `<pluginDir>/config` before. A version that is not one is
// taken as new, the place every release reads from now on.
function place(version, pluginDir, dataDir) {
  const c = compare(version, MOVED_AT);
  return c !== null && c < 0 ? path.join(pluginDir, 'config') : path.join(dataDir, 'config');
}

function readOr(file, fallback) {
  try { return fs.readFileSync(file, 'utf8'); } catch (e) { return fallback; }
}

// Whether a file in the plugin folder is a live configuration: present,
// and not the template's own text.
function live(file, templateFile) {
  const text = readOr(file, null);
  if (text === null) return false;
  return text !== readOr(templateFile, null);
}

// Make `dir` hold both files, at the plugin's start. For each: a live
// file in `pluginConfigDir` wins, is moved into `dir` and removed there
// (an older release or a tool wrote it since the last start: the newer
// truth); else a file in `dir` is kept; else the template is copied in.
// Returns what was done per name: `moved`, `kept` or `template`.
function ensure(dir, pluginConfigDir, templatesDir) {
  fs.mkdirSync(dir, { recursive: true });
  const done = {};
  NAMES.forEach(function (name) {
    const target = path.join(dir, name + '.txt');
    const template = path.join(templatesDir, name + '.txt.tmpl');
    const old = path.join(pluginConfigDir, name + '.txt');
    if (path.resolve(old) !== path.resolve(target) && live(old, template)) {
      fs.copyFileSync(old, target);
      fs.unlinkSync(old);
      done[name] = 'moved';
    } else if (fs.existsSync(target)) {
      done[name] = 'kept';
    } else {
      fs.copyFileSync(template, target);
      done[name] = 'template';
    }
  });
  return done;
}

// The files copied from `from` into `keepDir`, before an install; a file
// not there is not kept. Returns the names kept.
function keep(from, keepDir) {
  fs.mkdirSync(keepDir, { recursive: true });
  const kept = [];
  NAMES.forEach(function (name) {
    try {
      fs.copyFileSync(path.join(from, name + '.txt'), path.join(keepDir, name + '.txt'));
      kept.push(name);
    } catch (e) { /* not there */ }
  });
  return kept;
}

// The kept files put into `to`, the place of the release just installed,
// after its install script has run. Returns the names put back.
function restore(keepDir, to) {
  fs.mkdirSync(to, { recursive: true });
  const back = [];
  NAMES.forEach(function (name) {
    try {
      fs.copyFileSync(path.join(keepDir, name + '.txt'), path.join(to, name + '.txt'));
      back.push(name);
    } catch (e) { /* none kept */ }
  });
  return back;
}

module.exports = { NAMES, MOVED_AT, compare, place, live, ensure, keep, restore };
