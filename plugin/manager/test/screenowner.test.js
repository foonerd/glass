'use strict';
// The screen owner's planners: what a take changes and in what order, and
// what the way back restores, only where things are still as the take
// left them.
const test = require('node:test');
const assert = require('node:assert/strict');
const owner = require('../screenowner');

const pi = {
  now_playing: { installed: false },
  touch_display: { installed: true, enabled: true, status: 'STARTED' },
  display_configuration: { installed: false }
};

test('a take on a Pi turns the Touch Display plugin off, then the console the plugin brought back', () => {
  assert.deepEqual(owner.planTakePlugins(pi), [
    { kind: 'plugin', category: 'user_interface', name: 'touch_display', action: 'off', was: { enabled: true, status: 'STARTED' } }
  ]);
  // Read after the plugin stopped: its onStop enabled and started the console; the kiosk unit is gone.
  const units = { 'volumio-kiosk': { state: 'inactive', enabled: false }, 'getty@tty1': { state: 'active', enabled: true } };
  assert.deepEqual(owner.planTakeUnits(units), [
    { kind: 'unit', name: 'getty@tty1', action: 'stop', was: { active: true, enabled: true } }
  ]);
});

test('a take with Now Playing turns it off before the Touch Display plugin', () => {
  const plugins = Object.assign({}, pi, { now_playing: { installed: true, enabled: true, status: 'STARTED' } });
  assert.deepEqual(owner.planTakePlugins(plugins).map(s => s.name), ['now_playing', 'touch_display']);
});

test('a take on an image with its own kiosk stops and disables the unit, and Display Configuration goes off', () => {
  const plugins = { now_playing: { installed: false }, touch_display: { installed: false }, display_configuration: { installed: true, enabled: true, status: 'STARTED' } };
  assert.deepEqual(owner.planTakePlugins(plugins).map(s => s.name), ['display_configuration']);
  const units = { 'volumio-kiosk': { state: 'active', enabled: true }, 'getty@tty1': { state: 'inactive', enabled: false } };
  assert.deepEqual(owner.planTakeUnits(units), [
    { kind: 'unit', name: 'volumio-kiosk', action: 'stop', was: { active: true, enabled: true } }
  ]);
});

test('a plugin installed but off, and a unit neither running nor enabled, are not touched', () => {
  assert.deepEqual(owner.planTakePlugins({ touch_display: { installed: true, enabled: false, status: 'STOPPED' } }), []);
  assert.deepEqual(owner.planTakeUnits({ 'volumio-kiosk': { state: 'failed', enabled: false } }), []);
  assert.deepEqual(owner.planTakePlugins({}), []);
  assert.deepEqual(owner.planTakeUnits({}), []);
});

test('the way back restores in reverse order, only what is still as the take left it', () => {
  const register = {
    owner: 'glass-evo',
    changes: [
      { kind: 'plugin', category: 'user_interface', name: 'now_playing', action: 'off', was: { enabled: true, status: 'STARTED' } },
      { kind: 'plugin', category: 'user_interface', name: 'touch_display', action: 'off', was: { enabled: true, status: 'STARTED' } },
      { kind: 'unit', name: 'getty@tty1', action: 'stop', was: { active: true, enabled: true } }
    ]
  };
  const asLeft = {
    plugins: { now_playing: { installed: true, enabled: false, status: 'STOPPED' }, touch_display: { installed: true, enabled: false, status: 'STOPPED' } },
    units: { 'getty@tty1': { state: 'inactive', enabled: false } }
  };
  assert.deepEqual(owner.planGiveBack(register, asLeft), [
    { kind: 'unit', name: 'getty@tty1', action: 'start', enable: true, start: true },
    { kind: 'plugin', category: 'user_interface', name: 'touch_display', action: 'on' },
    { kind: 'plugin', category: 'user_interface', name: 'now_playing', action: 'on' }
  ]);
  // The user turned Now Playing on again themselves, and uninstalled nothing: it is left alone.
  const meddled = { plugins: Object.assign({}, asLeft.plugins, { now_playing: { installed: true, enabled: true, status: 'STARTED' } }), units: asLeft.units };
  assert.deepEqual(owner.planGiveBack(register, meddled).map(s => s.name), ['getty@tty1', 'touch_display']);
  // A plugin uninstalled in between, or a unit enabled again by hand, is skipped.
  const gone = { plugins: { touch_display: { installed: true, enabled: false, status: 'STOPPED' } }, units: { 'getty@tty1': { state: 'inactive', enabled: true } } };
  assert.deepEqual(owner.planGiveBack(register, gone).map(s => s.name), ['touch_display']);
  assert.deepEqual(owner.planGiveBack(null, asLeft), []);
});

test('a take that found a plugin off does not turn it on on the way back', () => {
  const register = { owner: 'glass-evo', changes: [{ kind: 'plugin', category: 'user_interface', name: 'touch_display', action: 'off', was: { enabled: false, status: 'STARTED' } }] };
  assert.deepEqual(owner.planGiveBack(register, { plugins: { touch_display: { installed: true, enabled: false, status: 'STOPPED' } } }), []);
});

test('the register says who owns the screen', () => {
  assert.equal(owner.ownedByEvo({ owner: 'glass-evo', takenAt: 1 }), true);
  assert.equal(owner.ownedByEvo({ owner: 'glass-evo', takenAt: 1, gaveBackAt: 2 }), false);
  assert.equal(owner.ownedByEvo(null), false);
});
