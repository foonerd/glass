'use strict';
// The engine's two files across Volumio's plugin lifecycle, replayed on
// real temporary trees in Volumio's own order (read from its plugin
// manager): the old plugin's stop, the plugin folder removed whole, the new
// folder moved in, the new install script (which makes a folder and no
// file), the Manager's restore on its paths, the backend restart, the new
// plugin's start. The plugin's real code runs at every moment where it
// exists; the recorded effect stands in for Volumio and for an older
// release's code. One test per path of the state table.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const engine = require('../enginefiles');
const themefolder = require('../themefolder');

const TEMPLATE = { meter: '[current]\nmeter = random\nmeter.folder = 800x480\n', spectrum: '[current]\nspectrum.folder = 800x480\n' };
const LIVE = { meter: '[current]\nmeter = One\nmeter.folder = 1280x720_mine\n', spectrum: '[current]\nspectrum.folder = 1280x720_mine\n' };
const LATER = { meter: '[current]\nmeter = Two\nmeter.folder = 1280x720_later\n', spectrum: '[current]\nspectrum.folder = 1280x720_later\n' };

// A player: the plugin folder, the data folder, config.json, and the
// Manager's keep folder.
function player() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-lifecycle-'));
  return { root, plugin: path.join(root, 'plugins', 'glass'), data: path.join(root, 'INTERNAL', 'glass'), keep: path.join(root, 'INTERNAL', 'glass', 'upgrade', 'config') };
}
const read = (f) => { try { return fs.readFileSync(f, 'utf8'); } catch (e) { return null; } };
const write = (f, t) => { fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, t); };

// A release's zip as a tree: templates in config/, nothing else that matters here.
function zipTree(p) {
  const dir = fs.mkdtempSync(path.join(p.root, 'zip-'));
  write(path.join(dir, 'config', 'meter.txt.tmpl'), TEMPLATE.meter);
  write(path.join(dir, 'config', 'spectrum.txt.tmpl'), TEMPLATE.spectrum);
  return dir;
}

// Volumio's updatePlugin, as its plugin manager does it: the old plugin
// stopped (nothing of ours there), the folder removed whole, the new tree
// moved in, the new install script run. The install script of an old
// release made the files from the templates when absent; the new one makes
// only the data folder's config directory.
function volumioUpdate(p, tree, release) {
  fs.rmSync(p.plugin, { recursive: true, force: true });
  fs.cpSync(tree, p.plugin, { recursive: true });
  if (engine.compare(release, engine.MOVED_AT) < 0) {
    engine.NAMES.forEach(function (name) {
      const f = path.join(p.plugin, 'config', name + '.txt');
      if (!fs.existsSync(f)) fs.copyFileSync(path.join(p.plugin, 'config', name + '.txt.tmpl'), f);
    });
  } else {
    fs.mkdirSync(path.join(p.data, 'config'), { recursive: true });
  }
}

// The Manager's apply around Volumio's update, as the RUNNING release's
// updater does it: an old one keeps from the plugin folder and puts back
// into the plugin folder whatever it installed (its code knows no other
// place); a new one keeps from the running release's place and puts back
// where the installed release reads.
function managerUpdate(p, running, tree, release) {
  const old = engine.compare(running, engine.MOVED_AT) < 0;
  const from = old ? path.join(p.plugin, 'config') : engine.place(running, p.plugin, p.data);
  engine.keep(from, p.keep);
  volumioUpdate(p, tree, release);
  engine.restore(p.keep, old ? path.join(p.plugin, 'config') : engine.place(release, p.plugin, p.data));
}

// The new plugin's start, as far as the files go.
function newStart(p) {
  return engine.ensure(path.join(p.data, 'config'), path.join(p.plugin, 'config'), path.join(p.plugin, 'config'));
}

// An old release standing on a player: its files live in its own folder.
function oldPlayer(files) {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.41');
  write(path.join(p.plugin, 'config', 'meter.txt'), files.meter);
  write(path.join(p.plugin, 'config', 'spectrum.txt'), files.spectrum);
  return p;
}
const dataFiles = (p) => ({ meter: read(path.join(p.data, 'config', 'meter.txt')), spectrum: read(path.join(p.data, 'config', 'spectrum.txt')) });
const pluginFiles = (p) => ({ meter: read(path.join(p.plugin, 'config', 'meter.txt')), spectrum: read(path.join(p.plugin, 'config', 'spectrum.txt')) });
const done = (p) => fs.rmSync(p.root, { recursive: true, force: true });

test('1. a fresh install of a new release makes the files from the templates at the start', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42');
  assert.deepEqual(dataFiles(p), { meter: null, spectrum: null }, 'the install script made no file');
  assert.deepEqual(newStart(p), { meter: 'template', spectrum: 'template' });
  assert.deepEqual(dataFiles(p), TEMPLATE);
  assert.equal(themefolder.wanted('800x480', '1280x720_chosen'), true, 'the theme of config.json goes back over the template');
  done(p);
});

test('2. old to new through Volumio\'s page: the live files are gone before any new code runs, the selection lost once, the theme put back', () => {
  const p = oldPlayer(LIVE);
  volumioUpdate(p, zipTree(p), '0.9.42');
  assert.deepEqual(pluginFiles(p), { meter: null, spectrum: null }, 'gone with the folder');
  assert.deepEqual(newStart(p), { meter: 'template', spectrum: 'template' });
  assert.deepEqual(dataFiles(p), TEMPLATE);
  assert.equal(themefolder.wanted('800x480', '1280x720_mine'), true);
  done(p);
});

test('3. old to new through the Manager: the old updater puts the live files back into the plugin folder, the new start takes them over', () => {
  const p = oldPlayer(LIVE);
  managerUpdate(p, '0.9.41', zipTree(p), '0.9.42');
  assert.deepEqual(pluginFiles(p), LIVE, 'back in the plugin folder, where the old updater puts them');
  assert.deepEqual(dataFiles(p), { meter: null, spectrum: null });
  assert.deepEqual(newStart(p), { meter: 'moved', spectrum: 'moved' });
  assert.deepEqual(dataFiles(p), LIVE);
  assert.deepEqual(pluginFiles(p), { meter: null, spectrum: null }, 'and gone from the plugin folder');
  done(p);
});

test('4. new to newer through Volumio\'s page: the data folder is not touched', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LIVE.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LIVE.spectrum);
  volumioUpdate(p, zipTree(p), '0.9.43');
  assert.deepEqual(newStart(p), { meter: 'kept', spectrum: 'kept' });
  assert.deepEqual(dataFiles(p), LIVE);
  done(p);
});

test('5. new to newer through the Manager: kept and put back into the same place, unchanged', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LIVE.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LIVE.spectrum);
  managerUpdate(p, '0.9.42', zipTree(p), '0.9.43');
  assert.deepEqual(newStart(p), { meter: 'kept', spectrum: 'kept' });
  assert.deepEqual(dataFiles(p), LIVE);
  done(p);
});

test('6. new back to old through the Manager, then forward again: the old release reads its folder, and its later changes win over the stale data folder', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LIVE.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LIVE.spectrum);
  managerUpdate(p, '0.9.42', zipTree(p), '0.9.41');
  assert.deepEqual(pluginFiles(p), LIVE, 'the old release reads the files as they stood');
  assert.deepEqual(dataFiles(p), LIVE, 'the data folder stays, stale from now on');
  // The user changes settings on the old release.
  write(path.join(p.plugin, 'config', 'meter.txt'), LATER.meter); write(path.join(p.plugin, 'config', 'spectrum.txt'), LATER.spectrum);
  managerUpdate(p, '0.9.41', zipTree(p), '0.9.42');
  assert.deepEqual(newStart(p), { meter: 'moved', spectrum: 'moved' });
  assert.deepEqual(dataFiles(p), LATER, 'the later changes won');
  done(p);
});

test('7. newer back to new through the Manager: the data folder throughout', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.43'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LIVE.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LIVE.spectrum);
  managerUpdate(p, '0.9.43', zipTree(p), '0.9.42');
  assert.deepEqual(newStart(p), { meter: 'kept', spectrum: 'kept' });
  assert.deepEqual(dataFiles(p), LIVE);
  done(p);
});

test('8. the way back to an old stable release: the plan written where the new release reads, carried into the old release\'s folder', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  // stableWrite: the planned files into the running release's place.
  write(path.join(p.data, 'config', 'meter.txt'), LATER.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LATER.spectrum);
  managerUpdate(p, '0.9.42', zipTree(p), '0.9.41');
  assert.deepEqual(pluginFiles(p), LATER, 'the old stable release reads the plan');
  done(p);
});

test('9. the way back to a new stable release: the plan stays in the data folder', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.43'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LATER.meter); write(path.join(p.data, 'config', 'spectrum.txt'), LATER.spectrum);
  managerUpdate(p, '0.9.43', zipTree(p), '0.9.42');
  assert.deepEqual(newStart(p), { meter: 'kept', spectrum: 'kept' });
  assert.deepEqual(dataFiles(p), LATER);
  done(p);
});

test('11. an older installer writing into the plugin folder on a new system: the live files win at the start', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  write(path.join(p.plugin, 'config', 'meter.txt'), LATER.meter); write(path.join(p.plugin, 'config', 'spectrum.txt'), LATER.spectrum);
  assert.deepEqual(newStart(p), { meter: 'moved', spectrum: 'moved' });
  assert.deepEqual(dataFiles(p), LATER);
  done(p);
});

test('12. an uninstall removes the data folder\'s configuration with the themes; a reinstall starts from the templates', () => {
  const p = player();
  volumioUpdate(p, zipTree(p), '0.9.42'); newStart(p);
  write(path.join(p.data, 'config', 'meter.txt'), LIVE.meter);
  // uninstall.sh's effect on the data folder's config.
  fs.rmSync(path.join(p.data, 'config'), { recursive: true, force: true });
  volumioUpdate(p, zipTree(p), '0.9.42');
  assert.deepEqual(newStart(p), { meter: 'template', spectrum: 'template' });
  done(p);
});

test('the new install script makes the data folder\'s config directory and no file', () => {
  const script = fs.readFileSync(path.join(__dirname, '..', '..', 'install.sh'), 'utf8');
  const lines = script.split('\n').filter((l) => !/^\s*#/.test(l));
  assert.ok(lines.some((l) => /mkdir -p "\$DATA_DIR\/config"/.test(l)), 'the folder is made');
  assert.ok(!lines.some((l) => /cp .*config\/(meter|spectrum|\$name)\.txt/.test(l)), 'no configuration file is copied by the install script');
});

test('the uninstall script removes the data folder\'s configuration', () => {
  const script = fs.readFileSync(path.join(__dirname, '..', '..', 'uninstall.sh'), 'utf8');
  assert.ok(/rm -rf "\$dir\/config"/.test(script), 'config/ goes with the uninstall');
});
