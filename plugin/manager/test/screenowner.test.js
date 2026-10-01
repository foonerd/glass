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

test('glass-evo can hold a screen the kernel drives, and no other', () => {
  assert.equal(owner.holdable({ connectors: [{ name: 'DSI-2', status: 'connected' }, { name: 'HDMI-A-1', status: 'disconnected' }] }), true);
  assert.equal(owner.holdable({ connectors: [{ name: 'LVDS-1', status: 'unknown' }] }), true, 'a panel that does not say is not ruled out');
  assert.equal(owner.holdable({ connectors: [{ name: 'HDMI-A-1', status: 'disconnected' }] }), false, 'nothing plugged in');
  assert.equal(owner.holdable({ connectors: [] }), false, 'a card with no kernel driver: the picture comes through X alone');
  assert.equal(owner.holdable(null), false);
});

test('an x86 player keeps X for the face; elsewhere the screen itself, X where the kernel drives none', () => {
  const live = { connectors: [{ name: 'DSI-2', status: 'connected' }] };
  const none = { connectors: [] };
  assert.equal(owner.holdMode('x64', none, true), 'x', 'a card with no kernel driver');
  assert.equal(owner.holdMode('x64', live, true), 'x', 'X all the same on x86');
  assert.equal(owner.holdMode('x64', live, false), 'kms', 'an x86 image with no X installed');
  assert.equal(owner.holdMode('arm', live, true), 'kms', 'a Pi with the Touch Display plugin\'s X installed draws on the screen itself');
  assert.equal(owner.holdMode('arm', none, true), 'x');
  assert.equal(owner.holdMode('arm', none, false), null, 'nothing to draw on');
  assert.equal(owner.holdMode('x64', none, false), null);
});

test('the way back stops the X server of the face before the kiosk starts its own', () => {
  const register = { owner: 'glass-evo', changes: [
    { kind: 'unit', name: 'volumio-kiosk', action: 'stop', was: { active: true, enabled: true } },
    { kind: 'unit', name: 'getty@tty1', action: 'stop', was: { active: true, enabled: true } },
    { kind: 'own-x', name: owner.OWN_X_UNIT, action: 'start' }
  ] };
  const steps = owner.planGiveBack(register, { plugins: {}, units: { 'volumio-kiosk': { state: 'inactive', enabled: false }, 'getty@tty1': { state: 'inactive', enabled: false } } });
  assert.deepEqual(steps.map((s) => s.kind + ' ' + s.name + ' ' + s.action), ['own-x glass-x stop', 'unit getty@tty1 start', 'unit volumio-kiosk start']);
});

test('the face\'s failures in a row are counted, and cleared by a run that holds', () => {
  const quick = { clean: false, ranMs: 800, windowMs: 10000 };
  assert.equal(owner.faceFailures(0, quick), 1);
  assert.equal(owner.faceFailures(2, quick), 3);
  assert.equal(owner.faceFailures(2, { clean: true, ranMs: 800, windowMs: 10000 }), 0, 'a relaunch is not a failure');
  assert.equal(owner.faceFailures(2, { clean: false, ranMs: 10000, windowMs: 10000 }), 0, 'it ran: a later death starts the count again');
});

test('the screen goes back by itself only where glass-evo owns it and cannot hold it', () => {
  const holds = { owner: 'glass-evo', available: true, failures: 0, stopped: false, enabled: true };
  assert.equal(owner.guard(holds), null);
  assert.equal(owner.guard(Object.assign({}, holds, { failures: owner.FACE_TRIES - 1 })), null);
  assert.equal(owner.guard(Object.assign({}, holds, { failures: owner.FACE_TRIES })), 'face-failed');
  assert.equal(owner.guard(Object.assign({}, holds, { available: false })), 'component-missing');
  assert.equal(owner.guard(Object.assign({}, holds, { stopped: true, enabled: false })), 'plugin-stopped');
  assert.equal(owner.guard(Object.assign({}, holds, { stopped: true, enabled: true })), null, 'an update stops the plugin, leaves it enabled and starts it again');
  assert.equal(owner.guard(Object.assign({}, holds, { stopped: true, enabled: true, available: false, failures: 9 })), null, 'a stopped plugin judges nothing else');
  assert.equal(owner.guard({ owner: 'kiosk', available: false, failures: 9, stopped: true, enabled: false }), null, 'the kiosk\'s screen is not Glass\'s to hand anywhere');
});

test('glass-evo is here where its component is installed or the screen is its own', () => {
  assert.equal(owner.here(null, { installed: false }), false, 'a player that never had it');
  assert.equal(owner.here({ owner: 'glass-evo', takenAt: 1, gaveBackAt: 2 }, { installed: false }), false, 'a register of the past alone is not it');
  assert.equal(owner.here(null, { installed: true, available: false }), true, 'installed, even with no binary for this player');
  assert.equal(owner.here({ owner: 'glass-evo', takenAt: 1 }, { installed: false }), true, 'the screen its own with the component gone: the way back must show');
  assert.equal(owner.here(null, undefined), false);
});
