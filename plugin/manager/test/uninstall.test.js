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

// The screen back before the data goes: the register's changes reversed
// through a systemctl of the test's, the face's X server first, the kiosk
// plugins noted for the next install, the register marked into the backups.
function withRegister(register) {
  const p = player(false, false);
  fs.writeFileSync(path.join(p.data, 'screen-owner.json'), JSON.stringify(register));
  const bin = path.join(p.dir, 'bin');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'systemctl'), '#!/bin/sh\necho "$@" >> "$SYSTEMCTL_LOG"\n');
  fs.chmodSync(path.join(bin, 'systemctl'), 0o755);
  p.log = path.join(p.dir, 'systemctl.log');
  p.env = Object.assign({}, process.env, { GLASS_UNINSTALL_FUNCTIONS: '1', SYSTEMCTL: path.join(bin, 'systemctl'), SYSTEMCTL_LOG: p.log, X0_SOCKET: path.join(p.dir, 'no-such-socket'), X0_WAIT_S: '0' });
  return p;
}
function giveBack(p) {
  return execFileSync('/bin/bash', ['-c', '. "$0"; give_screen_back "$1"', SCRIPT, p.data], { env: p.env, encoding: 'utf8' });
}

test('an uninstall gives the screen back from the register before the data goes', () => {
  const p = withRegister({ owner: 'glass-evo', takenAt: '2026-10-04T19:15:12Z', changes: [
    { kind: 'plugin', category: 'user_interface', name: 'touch_display', action: 'off', was: { enabled: true, status: 'STARTED' } },
    { kind: 'unit', name: 'volumio-kiosk', action: 'stop', was: { active: true, enabled: true } },
    { kind: 'unit', name: 'getty@tty1', action: 'stop', was: { active: false, enabled: false } },
    { kind: 'own-x', name: 'glass-x', action: 'start' }
  ] });
  const said = giveBack(p);
  assert.match(said, /Giving the screen back to the kiosk/);
  const calls = fs.readFileSync(p.log, 'utf8').trim().split('\n');
  assert.deepEqual(calls, ['stop glass-x', 'disable glass-x', 'reset-failed glass-x', 'enable volumio-kiosk', 'start volumio-kiosk', 'is-enabled volumio-kiosk', 'is-active volumio-kiosk'], 'the face\'s X server first, then the kiosk as it was, then a look whether an enabled kiosk runs; a unit that was off stays off');
  assert.match(said, /for the next install to turn on: touch_display/);
  assert.ok(!fs.existsSync(path.join(p.data, 'screen-owner.json')), 'the register is gone from its place');
  const note = JSON.parse(fs.readFileSync(path.join(p.data, 'backups', 'screen-owner.last.json'), 'utf8'));
  assert.strictEqual(note.gaveBackBy, 'uninstall');
  assert.ok(note.gaveBackAt);
  assert.deepEqual(note.pluginsPending, ['touch_display']);
  assert.strictEqual(note.changes.length, 4, 'the register travels whole');
  // Then the data goes and the note stays with the backups.
  clear(p.data);
  assert.ok(fs.existsSync(path.join(p.data, 'backups', 'screen-owner.last.json')));
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('a register already given back, or none, moves nothing', () => {
  const p = withRegister({ owner: 'glass-evo', takenAt: 't', gaveBackAt: 'g', changes: [{ kind: 'own-x', name: 'glass-x', action: 'start' }] });
  const said = giveBack(p);
  assert.ok(!/Giving the screen back/.test(said));
  assert.ok(!fs.existsSync(p.log), 'no systemctl call');
  const note = JSON.parse(fs.readFileSync(path.join(p.data, 'backups', 'screen-owner.last.json'), 'utf8'));
  assert.strictEqual(note.gaveBackAt, 'g', 'the earlier way back stands');
  assert.deepEqual(note.pluginsPending, []);
  fs.rmSync(p.dir, { recursive: true, force: true });
  const q = player(false, false);
  fs.unlinkSync(path.join(q.data, 'screen-owner.json'));
  const quiet = execFileSync('/bin/bash', ['-c', '. "$0"; give_screen_back "$1"', SCRIPT, q.data], { env: Object.assign({}, process.env, { GLASS_UNINSTALL_FUNCTIONS: '1' }), encoding: 'utf8' });
  assert.strictEqual(quiet, '');
  fs.rmSync(q.dir, { recursive: true, force: true });
});
