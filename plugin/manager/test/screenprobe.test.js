'use strict';
// The screen probe on texts captured from a player: a Raspberry Pi 5 with
// a portrait DSI panel, a Goodix touch panel, two HDMI CEC remotes and a
// backlight, the kiosk holding the screen.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const probe = require('../screenprobe');

const fixture = name => fs.readFileSync(path.join(__dirname, 'fixtures', 'probe', name), 'utf8');

test('the input devices are read with their event nodes and told apart', () => {
  const devices = probe.parseInputDevices(fixture('input-devices.txt'));
  assert.equal(devices.length, 6);
  const goodix = devices.find(d => /Goodix/.test(d.name));
  assert.equal(goodix.event, '/dev/input/event5');
  const udev = probe.parseUdev(fixture('udev-input.txt'));
  assert.equal(udev['/dev/input/event5'].ID_INPUT_TOUCHSCREEN, '1');
  const inputs = probe.classifyInputs(devices, udev);
  assert.deepEqual(inputs.touch.map(t => t.name), ['Goodix Capacitive TouchScreen']);
  assert.deepEqual(inputs.mice, [], 'a CEC remote is not a mouse');
  assert.deepEqual(inputs.remotes.map(r => r.name), ['vc4-hdmi-0', 'vc4-hdmi-1']);
  assert.deepEqual(inputs.keyboards, []);
});

test('the connectors are read with their native mode, the portrait panel told', () => {
  const connectors = probe.parseDrm(fixture('drm.txt'));
  assert.deepEqual(connectors.map(c => c.name), ['DSI-2', 'HDMI-A-1', 'HDMI-A-2'], 'writeback connectors are not screens');
  const dsi = connectors[0];
  assert.equal(dsi.status, 'connected');
  assert.equal(dsi.kind, 'dsi');
  assert.deepEqual(dsi.native, { width: 720, height: 1280 });
  assert.equal(dsi.portrait, true);
  assert.equal(connectors[1].status, 'disconnected');
  assert.equal(connectors[1].native, null);
});

test('the backlight is read and the suggestion follows the panel and the inputs', () => {
  const lights = probe.parseBacklights('## 11-0045\nbrightness=31\nactual_brightness=31\nmax_brightness=31\nbl_power=0\ntype=raw\n');
  assert.deepEqual(lights, [{ name: '11-0045', brightness: 31, max: 31, power: 0 }]);
  const found = {
    connectors: probe.parseDrm(fixture('drm.txt')),
    inputs: probe.classifyInputs(probe.parseInputDevices(fixture('input-devices.txt')), probe.parseUdev(fixture('udev-input.txt'))),
    touchDisplayAngle: 270
  };
  assert.deepEqual(probe.suggest(found), { rotation: 270, pointer: 'hide', panel: 'DSI-2' });
  assert.equal(probe.suggest(Object.assign({}, found, { touchDisplayAngle: null })).rotation, 270, 'a portrait panel with no angle on file suggests 270');
  assert.equal(probe.suggest(Object.assign({}, found, { touchDisplayAngle: 90 })).rotation, 90, 'the angle on file wins');
  const landscape = { connectors: [{ name: 'HDMI-A-1', status: 'connected', portrait: false }], inputs: { touch: [], mice: [{ name: 'a mouse' }], keyboards: [], remotes: [] } };
  assert.deepEqual(probe.suggest(landscape), { rotation: 0, pointer: 'show', panel: 'HDMI-A-1' });
  assert.equal(probe.pointerShown('auto', found), false);
  assert.equal(probe.pointerShown('auto', landscape), true);
  assert.equal(probe.pointerShown('show', found), true);
  assert.equal(probe.pointerShown('hide', landscape), false);
});

test('the screen is free only with no X, no kiosk running, starting or enabled, no plugin that brings one, and a panel', () => {
  const free = { xserver: false, kiosk: 'inactive', kioskEnabled: false, touchDisplay: false, displayConfiguration: false, panel: true };
  assert.equal(probe.screenFree(free), true);
  assert.equal(probe.screenFree(Object.assign({}, free, { xserver: true })), false, 'an X server holds it');
  assert.equal(probe.screenFree(Object.assign({}, free, { kiosk: 'activating' })), false, 'a kiosk on its way holds it');
  assert.equal(probe.screenFree(Object.assign({}, free, { kiosk: 'failed' })), true, 'a failed unit holds nothing');
  assert.equal(probe.screenFree(Object.assign({}, free, { kioskEnabled: true })), false, 'enabled at boot, it will come');
  assert.equal(probe.screenFree(Object.assign({}, free, { touchDisplay: true })), false, 'the plugin brings the kiosk');
  assert.equal(probe.screenFree(Object.assign({}, free, { displayConfiguration: true })), false, 'so does that one');
  assert.equal(probe.screenFree(Object.assign({}, free, { panel: false })), false, 'nothing to draw on');
  assert.equal(probe.screenFree({}), false);
  assert.equal(probe.wouldDraw(free), 'kmsdrm');
  assert.equal(probe.wouldDraw(Object.assign({}, free, { xserver: true, kiosk: 'active' })), 'x11');
  assert.equal(probe.wouldDraw(Object.assign({}, free, { touchDisplay: true })), null, 'no window while the kiosk is on its way');
});

test('gather reads the machine through its readers and never fails on what is missing', () => {
  const files = {
    '/proc/bus/input/devices': fixture('input-devices.txt'),
    '/proc/device-tree/model': 'Raspberry Pi 5 Model B Rev 1.1\u0000',
    '/data/configuration/plugins.json': JSON.stringify({ user_interface: { touch_display: { enabled: { type: 'boolean', value: true } }, glass: { enabled: { type: 'boolean', value: true } } } }),
    '/data/configuration/user_interface/touch_display/config.json': JSON.stringify({ angle: { type: 'string', value: '270' } }),
    '/sys/class/drm/card1-DSI-2/status': 'connected\n', '/sys/class/drm/card1-DSI-2/enabled': 'enabled\n', '/sys/class/drm/card1-DSI-2/modes': '720x1280\n', '/sys/class/drm/card1-DSI-2/edid': '',
    '/sys/class/backlight/11-0045/brightness': '31\n', '/sys/class/backlight/11-0045/max_brightness': '31\n', '/sys/class/backlight/11-0045/bl_power': '0\n'
  };
  const udev = probe.parseUdev(fixture('udev-input.txt'));
  const found = probe.gather({
    readFile(p) { if (files[p] === undefined) throw new Error('no ' + p); return files[p]; },
    readDir(p) { return p === '/sys/class/drm' ? ['card1-DSI-2', 'renderD128'] : p === '/sys/class/backlight' ? ['11-0045'] : []; },
    exists(p) { return p === '/tmp/.X11-unix/X0'; },
    exec(cmd, args) {
      if (cmd === 'udevadm') { const node = args[2].slice('--name='.length); return Object.entries(udev[node] || {}).map(([k, v]) => k + '=' + v).join('\n'); }
      if (cmd === 'systemctl') return 'active\n';
      throw new Error('no ' + cmd);
    }
  });
  assert.equal(found.board, 'Raspberry Pi 5 Model B Rev 1.1');
  assert.equal(found.connectors[0].name, 'DSI-2');
  assert.equal(found.inputs.touch.length, 1);
  assert.deepEqual(found.backlights[0], { name: '11-0045', brightness: 31, max: 31, power: 0 });
  assert.deepEqual(found.holders, { kiosk: true, xserver: true, touchDisplay: true, displayConfiguration: false });
  assert.equal(found.touchDisplayAngle, 270);
  assert.deepEqual(found.suggestion, { rotation: 270, pointer: 'hide', panel: 'DSI-2' });
  const bare = probe.gather({ readFile() { throw new Error('nothing'); }, readDir() { throw new Error('nothing'); }, exists() { return false; }, exec() { throw new Error('nothing'); } });
  assert.deepEqual(bare.connectors, []);
  assert.deepEqual(bare.inputs, { touch: [], mice: [], keyboards: [], remotes: [] });
  assert.deepEqual(bare.holders, { kiosk: false, xserver: false, touchDisplay: false, displayConfiguration: false });
  assert.deepEqual(bare.suggestion, { rotation: 0, pointer: 'hide', panel: null });
});

test('an X server brought up for the face is Glass\'s own screen while no kiosk is on it', () => {
  const own = { ownX: true, xserver: true, kiosk: 'inactive', kioskEnabled: false, touchDisplay: false, displayConfiguration: false, panel: false };
  assert.equal(probe.screenOwn(own), true);
  assert.equal(probe.screenFree(own), false, 'not free: an X server is up');
  assert.equal(probe.wouldDraw(own), 'x11');
  assert.equal(probe.screenOwn(Object.assign({}, own, { xserver: false })), false, 'its socket is not there yet');
  assert.equal(probe.screenOwn(Object.assign({}, own, { ownX: false })), false, 'a kiosk\'s X server');
  assert.equal(probe.screenOwn(Object.assign({}, own, { kiosk: 'activating' })), false, 'the kiosk is on its way');
  assert.equal(probe.screenOwn(Object.assign({}, own, { touchDisplay: true })), false);
  assert.equal(probe.screenOwn(null), false);
});

test('SDL is told the card with the connected screen, whatever order the cards come in', () => {
  const kmsCard = probe.kmsCard;
  // A Raspberry Pi 5 with a DSI panel and nothing on HDMI, as the kernel numbered it on two boots.
  const dsiFirst = { connectors: [{ card: 'card1', name: 'DSI-2', status: 'connected' }, { card: 'card2', name: 'HDMI-A-1', status: 'disconnected' }, { card: 'card2', name: 'HDMI-A-2', status: 'disconnected' }] };
  const dsiLast = { connectors: [{ card: 'card1', name: 'HDMI-A-1', status: 'disconnected' }, { card: 'card1', name: 'HDMI-A-2', status: 'disconnected' }, { card: 'card2', name: 'DSI-2', status: 'connected' }] };
  assert.equal(kmsCard(dsiFirst), 1);
  assert.equal(kmsCard(dsiLast), 2);
  assert.equal(kmsCard({ connectors: dsiFirst.connectors.slice().reverse() }), 1, 'the listing\'s order does not matter');
  // Two screens: the first by name, the same at every start.
  assert.equal(kmsCard({ connectors: [{ card: 'card2', name: 'HDMI-A-1', status: 'connected' }, { card: 'card1', name: 'DSI-2', status: 'connected' }] }), 1);
  // Nothing connected, or nothing known: no card is named and SDL searches as before.
  assert.equal(kmsCard({ connectors: [{ card: 'card1', name: 'HDMI-A-1', status: 'disconnected' }] }), null);
  assert.equal(kmsCard({}), null);
  assert.equal(kmsCard(undefined), null);
});
