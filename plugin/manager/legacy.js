'use strict';
// The old plugin's themes: what PeppyMeter Screensaver's data folder still
// holds after Glass copied them at its first install, and their removal.
// Only that folder's two theme trees are ever touched.

const fs = require('fs');
const path = require('path');

const LEGACY_NAME = 'peppy_screensaver';
const TREES = ['templates', 'templates_spectrum'];

function bytesUnder(dir) {
  let total = 0;
  let entries;
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch (e) { return 0; }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) total += bytesUnder(full);
    else if (entry.isFile()) { try { total += fs.statSync(full).size; } catch (e) { /* gone meanwhile */ } }
  }
  return total;
}

// What the old plugin's folder holds: whether its templates tree is there,
// how many theme folders it has, and the bytes of both trees.
function legacyThemes(legacyDir) {
  let entries;
  try { entries = fs.readdirSync(path.join(legacyDir, 'templates'), { withFileTypes: true }); } catch (e) {
    return { present: false, themes: 0, bytes: 0 };
  }
  const themes = entries.filter(function (e) { return e.isDirectory() && e.name.indexOf('.') !== 0; }).length;
  const bytes = TREES.reduce(function (sum, tree) { return sum + bytesUnder(path.join(legacyDir, tree)); }, 0);
  return { present: true, themes: themes, bytes: bytes };
}

// Remove the old plugin's two theme trees, nothing else; the folder must
// be the old plugin's own. The trees removed come back.
function wipeLegacyThemes(legacyDir) {
  if (path.basename(legacyDir) !== LEGACY_NAME) throw new Error('not the old plugin\'s folder: ' + legacyDir);
  const removed = [];
  for (const tree of TREES) {
    const full = path.join(legacyDir, tree);
    if (fs.existsSync(full)) {
      fs.rmSync(full, { recursive: true, force: true });
      removed.push(tree);
    }
  }
  return removed;
}

module.exports = { legacyThemes, wipeLegacyThemes, LEGACY_NAME, TREES };
