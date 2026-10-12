'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { place, live, ensure, keep, restore, compare } = require('../enginefiles');

function scratch() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-enginefiles-'));
  const plugin = path.join(root, 'plugin');
  const templates = path.join(plugin, 'config');
  fs.mkdirSync(templates, { recursive: true });
  fs.writeFileSync(path.join(templates, 'meter.txt.tmpl'), '[current]\nmeter.folder = 800x480\n');
  fs.writeFileSync(path.join(templates, 'spectrum.txt.tmpl'), '[current]\nspectrum.folder = 800x480\n');
  const data = path.join(root, 'data');
  return { root, plugin, templates, data, dir: path.join(data, 'config') };
}
const text = (f) => fs.readFileSync(f, 'utf8');

test('the place of the files is by release: the data folder from 0.9.42, the plugin folder before', () => {
  assert.equal(place('0.9.41', '/p', '/d'), path.join('/p', 'config'));
  assert.equal(place('0.9.42', '/p', '/d'), path.join('/d', 'config'));
  assert.equal(place('0.10.0', '/p', '/d'), path.join('/d', 'config'));
  assert.equal(place('1.0.0', '/p', '/d'), path.join('/d', 'config'));
  assert.equal(place('0.8.99', '/p', '/d'), path.join('/p', 'config'));
  assert.equal(place('', '/p', '/d'), path.join('/d', 'config'), 'no version: the place of every release from now on');
  assert.equal(compare('0.9.42', '0.9.41'), 1);
  assert.equal(compare('0.9.9', '0.9.41'), -1);
  assert.equal(compare('x', '0.9.41'), null);
});

test('the files are made from the templates when nothing is there, and kept once they are', () => {
  const s = scratch();
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'template', spectrum: 'template' });
  assert.equal(text(path.join(s.dir, 'meter.txt')), '[current]\nmeter.folder = 800x480\n');
  fs.writeFileSync(path.join(s.dir, 'meter.txt'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n');
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'kept', spectrum: 'kept' });
  assert.equal(text(path.join(s.dir, 'meter.txt')), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n', 'a kept file is not touched');
  fs.rmSync(s.root, { recursive: true, force: true });
});

test('a live file in the plugin folder wins over the data folder, is moved and removed there; the template text there is not live', () => {
  const s = scratch();
  fs.mkdirSync(s.dir, { recursive: true });
  fs.writeFileSync(path.join(s.dir, 'meter.txt'), '[current]\nmeter.folder = stale\n');
  fs.writeFileSync(path.join(s.templates, 'meter.txt'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n');
  fs.writeFileSync(path.join(s.templates, 'spectrum.txt'), '[current]\nspectrum.folder = 800x480\n');
  assert.equal(live(path.join(s.templates, 'meter.txt'), path.join(s.templates, 'meter.txt.tmpl')), true);
  assert.equal(live(path.join(s.templates, 'spectrum.txt'), path.join(s.templates, 'spectrum.txt.tmpl')), false, 'the template text itself');
  assert.equal(live(path.join(s.templates, 'none.txt'), path.join(s.templates, 'meter.txt.tmpl')), false);
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'moved', spectrum: 'template' });
  assert.equal(text(path.join(s.dir, 'meter.txt')), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n', 'the live file won over the stale one');
  assert.equal(fs.existsSync(path.join(s.templates, 'meter.txt')), false, 'and is gone from the plugin folder');
  assert.equal(fs.existsSync(path.join(s.templates, 'spectrum.txt')), true, 'the template copy is left where it was');
  fs.rmSync(s.root, { recursive: true, force: true });
});

test('the files are kept before an install and put back where the installed release reads them', () => {
  const s = scratch();
  fs.mkdirSync(s.dir, { recursive: true });
  fs.writeFileSync(path.join(s.dir, 'meter.txt'), 'M');
  const keepDir = path.join(s.root, 'keep');
  assert.deepEqual(keep(s.dir, keepDir), ['meter'], 'only what is there');
  assert.deepEqual(restore(keepDir, place('0.9.41', s.plugin, s.data)), ['meter']);
  assert.equal(text(path.join(s.plugin, 'config', 'meter.txt')), 'M', 'an old release reads the plugin folder');
  assert.deepEqual(restore(keepDir, place('0.9.43', s.plugin, s.data)), ['meter']);
  assert.equal(text(path.join(s.dir, 'meter.txt')), 'M', 'a new one the data folder');
  fs.rmSync(s.root, { recursive: true, force: true });
});
