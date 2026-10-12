'use strict';
// The engine's two configuration files, meter.txt and spectrum.txt, kept
// under the player's own data folder, out of the plugin's folder that
// every install replaces. Made here when absent: from a live file the
// plugin's folder still holds (an older release kept them there, and the
// Manager's own update puts them back there), else from the template.

const fs = require('fs');
const path = require('path');

const NAMES = ['meter', 'spectrum'];

function readOr(file, fallback) {
  try { return fs.readFileSync(file, 'utf8'); } catch (e) { return fallback; }
}

// Whether a file in the plugin's folder is a live configuration worth
// keeping: present, and not the template's own text.
function live(file, templateFile) {
  const text = readOr(file, null);
  if (text === null) return false;
  return text !== readOr(templateFile, null);
}

// Make `dir` hold both files. `templates` is the plugin's config folder
// with `<name>.txt.tmpl`; `previous` is where an older release kept the
// live files (the same folder). Returns what was done per name: `kept`
// (already there), `moved` (a live file taken over), or `template`.
function ensure(dir, templates, previous) {
  fs.mkdirSync(dir, { recursive: true });
  const done = {};
  NAMES.forEach(function (name) {
    const target = path.join(dir, name + '.txt');
    if (fs.existsSync(target)) { done[name] = 'kept'; return; }
    const template = path.join(templates, name + '.txt.tmpl');
    const old = previous ? path.join(previous, name + '.txt') : null;
    if (old && live(old, template)) {
      fs.copyFileSync(old, target);
      done[name] = 'moved';
    } else {
      fs.copyFileSync(template, target);
      done[name] = 'template';
    }
  });
  return done;
}

module.exports = { NAMES, ensure, live };
