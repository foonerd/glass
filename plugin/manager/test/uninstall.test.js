'use strict';
// What an uninstall leaves under Internal Storage: the settings backups
// always, everything where the user asked to keep the themes. The script's
// own function is run, on a folder of the test's.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

const SCRIPT = path.join(__dirname, '..', '..', 'uninstall.sh');

function player(preserve, backups) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-uninstall-'));
  const data = path.join(dir, 'glass');
  for (const d of ['templates/1280x720_amber', 'templates_spectrum/1280x720_amber', 'catalog/downloads', 'previews/x', 'evo/bin', 'faces/Mine']) fs.mkdirSync(path.join(data, d), { recursive: true });
  fs.writeFileSync(path.join(data, 'templates/1280x720_amber/meters.txt'), '[m]\n');
  fs.writeFileSync(path.join(data, 'screen-owner.json'), '{}');
  fs.mkdirSync(path.join(data, 'backups'), { recursive: true });
  if (backups) {
    fs.mkdirSync(path.join(data, 'backups', 'before-0.8.38'), { recursive: true });
    fs.writeFileSync(path.join(data, 'backups', 'before-0.8.38', 'config.json'), '{}');
  }
  if (preserve) fs.writeFileSync(path.join(data, '.preserve'), '');
  return { dir, data };
}

function clear(data) {
  return execFileSync('/bin/bash', ['-c', '. "$0"; clear_data "$1"', SCRIPT, data], { env: Object.assign({}, process.env, { GLASS_UNINSTALL_FUNCTIONS: '1' }), encoding: 'utf8' });
}

test('an uninstall keeps the settings backups and removes the rest', () => {
  const p = player(false, true);
  const said = clear(p.data);
  assert.deepEqual(fs.readdirSync(p.data), ['backups']);
  assert.ok(fs.existsSync(path.join(p.data, 'backups', 'before-0.8.38', 'config.json')));
  assert.match(said, /the settings backups stay/);
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('with nothing backed up the folder goes whole', () => {
  const p = player(false, false);
  clear(p.data);
  assert.equal(fs.existsSync(p.data), false);
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('where the user keeps the themes everything stays', () => {
  const p = player(true, true);
  const said = clear(p.data);
  assert.ok(fs.existsSync(path.join(p.data, 'templates/1280x720_amber/meters.txt')));
  assert.ok(fs.existsSync(path.join(p.data, 'backups', 'before-0.8.38', 'config.json')));
  assert.ok(fs.existsSync(path.join(p.data, 'evo/bin')));
  assert.match(said, /Keeping the themes and backups/);
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('a folder that is not there is no error, and sourcing for the functions runs nothing', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-uninstall-'));
  assert.equal(clear(path.join(dir, 'none')), '');
  fs.rmSync(dir, { recursive: true, force: true });
});
