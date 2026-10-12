'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { ensure, live } = require('../enginefiles');

function scratch() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-enginefiles-'));
  const templates = path.join(root, 'plugin', 'config');
  fs.mkdirSync(templates, { recursive: true });
  fs.writeFileSync(path.join(templates, 'meter.txt.tmpl'), '[current]\nmeter.folder = 800x480\n');
  fs.writeFileSync(path.join(templates, 'spectrum.txt.tmpl'), '[current]\nspectrum.folder = 800x480\n');
  return { root, templates, dir: path.join(root, 'data', 'config') };
}

test('the files are made from the templates when nothing is there, and kept once they are', () => {
  const s = scratch();
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'template', spectrum: 'template' });
  assert.equal(fs.readFileSync(path.join(s.dir, 'meter.txt'), 'utf8'), '[current]\nmeter.folder = 800x480\n');
  fs.writeFileSync(path.join(s.dir, 'meter.txt'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n');
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'kept', spectrum: 'kept' });
  assert.equal(fs.readFileSync(path.join(s.dir, 'meter.txt'), 'utf8'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n', 'a kept file is not touched');
  fs.rmSync(s.root, { recursive: true, force: true });
});

test('a live file an older release kept in the plugin folder is taken over; the template copy there is not', () => {
  const s = scratch();
  fs.writeFileSync(path.join(s.templates, 'meter.txt'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n');
  fs.writeFileSync(path.join(s.templates, 'spectrum.txt'), '[current]\nspectrum.folder = 800x480\n');
  assert.equal(live(path.join(s.templates, 'meter.txt'), path.join(s.templates, 'meter.txt.tmpl')), true);
  assert.equal(live(path.join(s.templates, 'spectrum.txt'), path.join(s.templates, 'spectrum.txt.tmpl')), false, 'the template text itself');
  assert.equal(live(path.join(s.templates, 'none.txt'), path.join(s.templates, 'meter.txt.tmpl')), false);
  assert.deepEqual(ensure(s.dir, s.templates, s.templates), { meter: 'moved', spectrum: 'template' });
  assert.equal(fs.readFileSync(path.join(s.dir, 'meter.txt'), 'utf8'), '[current]\nmeter.folder = 1280x720_mine\nmeter = One\n');
  fs.rmSync(s.root, { recursive: true, force: true });
});
