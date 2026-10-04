'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { SYMPTOMS, CHECKS, diagnose, lineTime } = require('../diagnose');
const strings = require('../../i18n/strings_en.json').GLASS;

const NOW = Date.parse('2026-10-01T05:00:00+01:00');

// A player in good order: playing a file, measured, the display up on the
// kiosk's X server, a touch panel, remotes served with none connected.
function healthy() {
  return {
    now: NOW,
    update: { current: '0.7.93', latest: { version: '0.7.93' }, available: false },
    log: [],
    status: {
      version: '0.7.93', arch: 'arm', binary: true, legacy: false, headless: false, running: true, timeout: 5,
      interactive: 'on', measured: true, diskFree: 900 * 1073741824, themeSize: '1280x720',
      channel: { clients: 1, status: 'play', service: 'mpd' },
      showing: { rate: 30 },
      artwork: { enabled: true },
      face: { pages: 0, frames: false },
      remotes: { enabled: true, serving: true, ports: { frames: 5580, channel: 5581 }, receiving: 0, connected: 0, problem: null },
      performance: { auto: 'standard', values: { frameRate: 30 }, profiles: { standard: { frameRate: 30 } }, board: { model: 'Raspberry Pi 5 Model B Rev 1.1' } },
      sheet: {
        player: { backendUptimeS: 600 },
        screen: { size: '1280x720' },
        audio: { tapInChain: true },
        housekeeping: { problems: [] }
      }
    },
    screen: {
      free: false, rotation: 270,
      fact: { xserver: true, kiosk: 'active', kioskEnabled: false, touchDisplay: true, displayConfiguration: false, panel: true },
      now: { display: { running: true, driver: 'x11', renderer: 'opengl' }, wouldDraw: 'x11' },
      probe: { inputs: { touch: [{ name: 'Goodix Capacitive TouchScreen', event: '/dev/input/event5' }], mice: [], keyboards: [], remotes: [] }, suggestion: { rotation: 270, pointer: 'hide', panel: 'DSI-2' } },
      touch: { mapping: 'auto', calibration: { state: 'none' } }
    }
  };
}

const kinds = (result) => result.findings.map((x) => x.kind + ':' + x.check);
const one = (result, check) => result.findings.find((x) => x.check === check);

test('a player in good order shows no cause for any symptom', function () {
  SYMPTOMS.forEach(function (symptom) {
    const result = diagnose(symptom, healthy());
    assert.strictEqual(result.found, false, symptom + ': ' + kinds(result).join(' '));
    assert.strictEqual(result.symptom, symptom);
  });
  assert.throws(() => diagnose('nonsense', healthy()), /unknown symptom/);
});

test('every finding a check can make has its words, with every value it fills in', function () {
  const source = require('fs').readFileSync(require.resolve('../diagnose'), 'utf8');
  const keys = Array.from(new Set(source.match(/'DIAG_[A-Z_]+'/g).map((k) => k.slice(1, -1))));
  assert.ok(keys.length > 30, 'the checks name their strings');
  keys.forEach((key) => assert.ok(strings['MANAGER_' + key], key + ' is in the strings'));
  SYMPTOMS.forEach((s) => assert.ok(strings['MANAGER_DIAG_S_' + s.toUpperCase().replace(/-/g, '_')], s + ' has a title'));
  CHECKS.forEach((c) => c.symptoms.forEach((s) => assert.ok(SYMPTOMS.includes(s), c.id + ' names a known symptom')));
  // Each finding of the healthy player and of the broken ones below fills
  // every {value} its string asks for.
  const broken = healthy();
  Object.assign(broken.status, { legacy: true, binary: false, diskFree: 5 * 1048576, interactive: 'off', measured: false });
  broken.update.available = true; broken.update.latest.version = '0.8.0';
  broken.status.sheet.housekeeping.problems = ['warn: glass: something'];
  [healthy(), broken].forEach(function (facts) {
    SYMPTOMS.forEach(function (symptom) {
      diagnose(symptom, facts).findings.forEach(function (x) {
        const text = strings['MANAGER_' + x.key];
        (text.match(/\{[a-z]+\}/g) || []).forEach((slot) => assert.ok(slot.slice(1, -1) in x.with, x.key + ' fills ' + slot));
      });
    });
  });
});

test('the meters never appear: not playing, no screen of its own, a kiosk that failed, nowhere to draw', function () {
  let f = healthy();
  f.status.channel.status = 'stop'; f.status.running = false;
  let r = diagnose('no-meters', f);
  assert.strictEqual(r.found, true);
  assert.deepStrictEqual(one(r, 'playing').with, { status: 'stop', timeout: 5 });
  assert.strictEqual(r.findings[0].kind, 'cause', 'causes come first');

  f = healthy(); f.status.headless = true; f.status.running = false;
  assert.strictEqual(one(diagnose('no-meters', f), 'output').key, 'DIAG_HEADLESS');

  f = healthy(); f.status.running = false; f.screen.fact.kiosk = 'failed'; f.screen.fact.xserver = false; f.screen.now = { display: { running: false }, wouldDraw: null };
  assert.strictEqual(one(diagnose('no-meters', f), 'somewhere').key, 'DIAG_KIOSK_FAILED');

  // Where glass-evo holds the screen the kiosk is not meant to run: its unit
  // left as failed by the take is not what is wrong, on any symptom.
  f = healthy(); f.screen.fact.kiosk = 'failed'; f.screen.fact.xserver = false; f.screen.free = true;
  f.screen.owner = { owner: 'glass-evo' }; f.screen.now = { display: { running: true, driver: 'kmsdrm' }, wouldDraw: 'kmsdrm' };
  ['no-meters', 'restarts', 'screen'].forEach(function (symptom) {
    assert.ok(!diagnose(symptom, f).findings.some(function (x) { return x.key === 'DIAG_KIOSK_FAILED'; }), symptom);
  });
  f.screen.owner = { owner: 'kiosk' };
  assert.strictEqual(one(diagnose('screen', f), 'somewhere').key, 'DIAG_KIOSK_FAILED', 'with the kiosk as owner it is');

  f = healthy(); f.status.running = false; f.screen.fact = { xserver: false, kiosk: 'inactive', panel: false }; f.screen.now = { display: { running: false }, wouldDraw: null };
  assert.strictEqual(one(diagnose('no-meters', f), 'somewhere').key, 'DIAG_NO_PANEL');

  f = healthy(); f.status.running = false; f.screen.fact = { xserver: false, kiosk: 'activating', panel: true }; f.screen.now = { display: { running: false }, wouldDraw: null };
  assert.deepStrictEqual(one(diagnose('no-meters', f), 'somewhere').with, { kiosk: 'activating' });

  f = healthy(); f.status.running = false;
  r = diagnose('no-meters', f);
  assert.strictEqual(r.found, false, 'playing with the display away is not explained by a check');
  assert.strictEqual(one(r, 'display').kind, 'note');
});

test('the journal of this run names a display that died; an earlier run is no evidence', function () {
  const f = healthy();
  f.log = [
    '2026-10-01T04:40:00+0100 player volumio[1]: warn: glass: the display died 1 s after launch; next attempt in 5 s',
    '2026-10-01T04:55:00+0100 player volumio[9]: info: glass: display started'
  ];
  assert.strictEqual(one(diagnose('restarts', f), 'deaths').kind, 'ok', 'the death was before this backend started (up 600 s)');
  f.log.push('2026-10-01T04:56:00+0100 player volumio[9]: warn: glass: the display died 2 s after launch; next attempt in 5 s');
  f.log.push('2026-10-01T04:57:00+0100 player volumio[9]: warn: glass: the display died 1 s after launch; next attempt in 10 s');
  const died = one(diagnose('restarts', f), 'deaths');
  assert.strictEqual(died.kind, 'cause');
  assert.deepStrictEqual(died.with, { count: 2, last: 'the display died 1 s after launch; next attempt in 10 s' });
  f.log.push('2026-10-01T04:58:00+0100 player volumio[9]: glass-launcher: the shipped binary for this machine is missing or is the wrong CPU.');
  assert.strictEqual(one(diagnose('no-meters', f), 'deaths').key, 'DIAG_LOG_WRONG_BINARY');
  assert.strictEqual(lineTime('2026-10-01T04:58:00+0100 x'), Date.parse('2026-10-01T04:58:00+01:00'));
  assert.ok(isNaN(lineTime('no stamp here')));
});

test('the meters show but do not move: the tap out of the chain, a source below the tap', function () {
  let f = healthy(); f.status.sheet.audio.tapInChain = false;
  assert.strictEqual(one(diagnose('not-moving', f), 'tap').kind, 'cause');
  f = healthy(); f.status.measured = false; f.status.channel.service = 'spop';
  const m = one(diagnose('not-moving', f), 'measured');
  assert.strictEqual(m.kind, 'cause');
  assert.deepStrictEqual(m.with, { service: 'spop' });
  f = healthy(); f.status.channel.status = 'pause';
  const r = diagnose('not-moving', f);
  assert.strictEqual(one(r, 'playing').kind, 'cause');
  assert.strictEqual(one(r, 'measured'), undefined, 'nothing is measured while nothing plays, and that is no finding');
});

test('touch and the controls: the setting, a panel to touch, the channel, the map', function () {
  let f = healthy(); f.status.interactive = 'off';
  assert.strictEqual(one(diagnose('touch', f), 'interactive').go, 'system');
  f = healthy(); f.status.interactive = 'theme';
  assert.strictEqual(one(diagnose('touch', f), 'interactive').kind, 'note');
  f = healthy(); f.screen.probe.inputs.touch = [];
  assert.strictEqual(one(diagnose('touch', f), 'pointing').kind, 'cause');
  f = healthy(); f.status.channel.clients = 0;
  assert.strictEqual(one(diagnose('touch', f), 'channel').kind, 'cause');
  f = healthy();
  assert.strictEqual(one(diagnose('touch', f), 'touch-map').key, 'DIAG_TOUCH_UNDER_KIOSK');
  f.screen.free = true; f.screen.touch.calibration = { state: 'failed', error: 'unfit' };
  assert.strictEqual(one(diagnose('touch', f), 'touch-map').kind, 'cause');
  f.screen.touch = { mapping: 'overrides', calibration: { state: 'none' } };
  assert.deepStrictEqual(one(diagnose('touch', f), 'touch-map').with, { mapping: 'overrides' });
});

test('the screen: whose it is, a portrait panel not turned, a theme of another size', function () {
  let f = healthy();
  let r = diagnose('screen', f);
  assert.strictEqual(one(r, 'whose').kind, 'note');
  assert.strictEqual(one(r, 'rotation'), undefined, 'the kiosk turns its own screen');
  f.screen.free = true; f.screen.rotation = 0;
  const turn = one(diagnose('screen', f), 'rotation');
  assert.strictEqual(turn.kind, 'cause');
  assert.deepStrictEqual(turn.with, { rotation: 0, suggested: 270, panel: 'DSI-2' });
  f.screen.rotation = 90;
  assert.strictEqual(one(diagnose('screen', f), 'rotation').kind, 'note');
  f.screen.rotation = 270;
  assert.strictEqual(one(diagnose('screen', f), 'rotation').kind, 'ok');
  f.status.themeSize = '1920x1080';
  assert.deepStrictEqual(one(diagnose('screen', f), 'size').with, { theme: '1920x1080', screen: '1280x720' });
});

test('remotes and the Face: serving, frames that do not arrive, pages without frames', function () {
  let f = healthy(); f.status.remotes.enabled = false;
  assert.strictEqual(one(diagnose('remotes', f), 'serving').key, 'DIAG_SERVING_OFF');
  f = healthy(); f.status.remotes.serving = false; f.status.remotes.problem = 'port 5580 is taken';
  assert.deepStrictEqual(one(diagnose('remotes', f), 'serving').with, { problem: 'port 5580 is taken' });
  f = healthy(); f.status.remotes.connected = 2; f.status.remotes.receiving = 1;
  assert.deepStrictEqual(one(diagnose('remotes', f), 'receiving').with, { connected: 2, receiving: 1, port: 5580 });
  f = healthy(); f.status.face = { pages: 1, frames: false };
  assert.strictEqual(one(diagnose('remotes', f), 'face').kind, 'cause');
  f.status.channel.status = 'pause';
  assert.strictEqual(one(diagnose('remotes', f), 'face').kind, 'note', 'nothing moves while nothing plays');
});

test('slow: a rate the display lowered, a rate above what suits the board, and what is common to all', function () {
  let f = healthy(); f.status.showing.rate = 15;
  assert.deepStrictEqual(one(diagnose('slow', f), 'governed').with, { rate: 15, set: 30 });
  f = healthy(); f.status.performance.values.frameRate = 60;
  assert.strictEqual(one(diagnose('slow', f), 'rate').kind, 'note');
  assert.strictEqual(one(diagnose('slow', healthy()), 'fanart').key, 'DIAG_FANART_COSTS');
  f = healthy(); f.status.legacy = true; f.status.binary = false; f.status.diskFree = 5 * 1048576; f.update.available = true; f.update.latest.version = '0.8.0';
  f.status.sheet.housekeeping.problems = ['a warning'];
  const r = diagnose('other', f);
  assert.deepStrictEqual(kinds(r), ['cause:legacy', 'cause:binary', 'cause:disk', 'note:release', 'note:journal']);
  // Facts that are missing make no finding and no crash.
  assert.deepStrictEqual(diagnose('touch', { status: {} }).findings, []);
});

test('graphics that cannot draw on the screen itself are named as the cause, with the reason found', () => {
  const find = function (graphics, symptom) {
    const f = healthy();
    f.screen = Object.assign({}, f.screen || {}, { graphics: graphics });
    return diagnose(symptom || 'restarts', f).findings.find(function (x) { return x.check === 'graphics'; });
  };
  const x = { state: 'unknown', detail: '' };
  // The case it was written for.
  const broken = find({ asked: true, applies: true, ok: false, reason: 'EGL gave no display for /dev/dri/card1 (EGL_SUCCESS); the EGL vendor library libEGL_mesa.so.0 would not load: libxshmfence.so.1: cannot open shared object file', x: { state: 'software', detail: 'eglGetDisplay() failed' } });
  assert.strictEqual(broken.kind, 'cause');
  assert.strictEqual(broken.key, 'DIAG_GRAPHICS_BROKEN');
  assert.ok(broken.with.reason.indexOf('libxshmfence.so.1') !== -1);
  assert.strictEqual(broken.go, 'screen');
  assert.ok(strings['MANAGER_' + broken.key].indexOf('{reason}') !== -1);
  // The probe passes but the kiosk's X server draws in software: worth knowing.
  const soft = find({ asked: true, applies: true, ok: true, reason: 'EGL works', x: { state: 'software', detail: 'glamor initialization failed' } });
  assert.deepStrictEqual([soft.kind, soft.key, soft.with.detail], ['note', 'DIAG_GRAPHICS_X_SOFTWARE', 'glamor initialization failed']);
  // Drawn in software by the processor: works, and is said.
  const cpu = find({ asked: true, applies: true, ok: true, software: true, renderer: 'llvmpipe (LLVM 15.0.6, 128 bits)', reason: 'EGL works', x: x }, 'screen');
  assert.deepStrictEqual([cpu.kind, cpu.key], ['note', 'DIAG_GRAPHICS_SOFTWARE']);
  // In order: checked and found right.
  const fine = find({ asked: true, applies: true, ok: true, software: false, renderer: 'V3D 7.1.10.2', reason: 'EGL works on the screen\'s device (EGL 1.5); rendered by V3D 7.1.10.2', x: { state: 'accelerated', detail: 'V3D 7.1.10.2' } }, 'no-meters');
  assert.deepStrictEqual([fine.kind, fine.key], ['ok', 'DIAG_GRAPHICS_OK']);
  // Not asked, or no screen of the kernel's: nothing said.
  assert.strictEqual(find({ asked: false }), undefined);
  assert.strictEqual(find({ asked: true, applies: false, ok: true, reason: 'no screen', x: x }), undefined);
  assert.strictEqual(find(undefined), undefined);
  // Another symptom does not ask.
  assert.strictEqual(find({ asked: true, applies: true, ok: false, reason: 'x', x: x }, 'artwork'), undefined);
  for (const key of ['DIAG_GRAPHICS_BROKEN', 'DIAG_GRAPHICS_X_SOFTWARE', 'DIAG_GRAPHICS_SOFTWARE', 'DIAG_GRAPHICS_OK']) assert.ok(strings['MANAGER_' + key], key);
});

test('a window the display could not open is found by what the display really writes', () => {
  const at = '2026-10-01T04:58:0';
  const line = (n, words) => at + n + '+0100 player volumio[9]: info: glass: ' + words;
  const find = function (lines) {
    const f = healthy();
    f.log = lines;
    return diagnose('restarts', f).findings.find(function (x) { return x.check === 'deaths'; });
  };
  // As a player wrote it: SDL's words, relayed, then the line that sums the death up.
  const egl = find([
    line(1, "SDL error: Can't load EGL/GL library on window creation."),
    at + '1+0100 player volumio[9]: error: glass: the display did not run: exit 1 glass: SDL error: Can\'t load EGL/GL library on window creation.',
    at + '1+0100 player volumio[9]: warn: glass: the display died 0 s after launch; next attempt in 1 s'
  ]);
  assert.deepStrictEqual([egl.kind, egl.key, egl.with.count], ['cause', 'DIAG_LOG_NO_WINDOW', 1]);
  assert.strictEqual(egl.with.last, "SDL error: Can't load EGL/GL library on window creation.");
  // An X server that does not admit the display, and a player with nothing to draw on.
  assert.strictEqual(find([line(1, 'x11 not available'), line(2, 'x11 not available')]).with.count, 2);
  assert.strictEqual(find([line(1, 'no screen to draw on: no X server, no Wayland and no KMS/DRM device (SDL fell back to offscreen)')]).key, 'DIAG_LOG_NO_WINDOW');
  // The sentence the check looked for until 0.8.42 was never written by anything.
  assert.notStrictEqual(find([line(1, 'something else entirely')]).key, 'DIAG_LOG_NO_WINDOW');
  assert.ok(strings.MANAGER_DIAG_LOG_NO_WINDOW.indexOf('{last}') !== -1);
});
