'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const { legacyThemes, wipeLegacyThemes } = require('../legacy');

test('the old plugin\'s themes are counted and measured, and only its two trees are wiped', async () => {
  const root = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-legacy-'));
  const legacy = path.join(root, 'peppy_screensaver');
  fs.mkdirSync(path.join(legacy, 'templates', '800x480'), { recursive: true });
  fs.mkdirSync(path.join(legacy, 'templates', '1280x400_x'), { recursive: true });
  fs.mkdirSync(path.join(legacy, 'templates', '.hidden'), { recursive: true });
  fs.mkdirSync(path.join(legacy, 'templates_spectrum', '800x480'), { recursive: true });
  fs.mkdirSync(path.join(legacy, 'backups'), { recursive: true });
  fs.writeFileSync(path.join(legacy, 'templates', '800x480', 'meters.txt'), 'x'.repeat(100));
  fs.writeFileSync(path.join(legacy, 'templates_spectrum', '800x480', 'spectrum.txt'), 'y'.repeat(50));
  fs.writeFileSync(path.join(legacy, 'backups', 'keep.zip'), 'z');
  assert.deepEqual(legacyThemes(legacy), { present: true, themes: 2, bytes: 150 });
  assert.throws(function () { wipeLegacyThemes(path.join(root, 'other')); }, /not the old plugin/);
  assert.deepEqual(wipeLegacyThemes(legacy), ['templates', 'templates_spectrum']);
  assert.equal(fs.existsSync(path.join(legacy, 'templates')), false);
  assert.equal(fs.existsSync(path.join(legacy, 'backups', 'keep.zip')), true, 'the backups stay');
  assert.deepEqual(legacyThemes(legacy), { present: false, themes: 0, bytes: 0 });
  assert.deepEqual(wipeLegacyThemes(legacy), [], 'nothing left to remove');
});
