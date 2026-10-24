'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { SYMPTOMS, CHECKS, diagnose, lineTime } = require('../diagnose');
const strings = require('../../i18n/strings_en.json').GLASS;

const NOW = Date.parse('2026-10-01T05:00:00+01:00');

// glass-evo in good order on the sheet: installed and paired, owning the
// screen with its own binary started after the install, the views drawing
// with its module, a shipped look, a picture that is there, a place with a
// fresh reading.
function healthyEvo() {
  return {
    component: { version: '0.2.4', available: true, requires: '0.9.3' }, least: '0.2.4', outdated: false, previous: null,
    owner: 'glass-evo', holdMode: 'kms',
    displayBinary: '/data/INTERNAL/glass/evo/bin/arm/glass-evo', displayStartedAt: '2026-10-01T03:00:00Z', componentInstalledAt: '2026-10-01T02:00:00Z', displayOlder: false,
    views: { mode: 'follow', face: true, has: true, version: '0.2.4' }, pages: 0,
    look: { name: 'Clear', source: 'shipped', skies: 0, themeBrings: false, themeFolder: null, themeSkies: 0, unknownSkies: [] },
    picture: { name: 'lake.jpg', found: true }, idleWait: 'none', persistS: 15, faceSize: 'normal', buttons: true, motion: true, smallBoard: false,
    weather: { on: true, place: 'Bergen, Vestland, Norway', unit: 'C', span: 'today', readingAgeS: 600, error: '' },
    problems: []
  };
}

// A player in good order: playing a file, measured, the display up on the
// kiosk's X server, a touch panel, remotes served with none connected, no
// install since the backend started, three sources to browse.
function healthy() {
  return {
    now: NOW,
    update: { current: '0.7.93', latest: { version: '0.7.93' }, available: false },
    log: [],
    job: null,
    held: { glass: null, evo: null },
    browse: { count: 3, names: ['Favourites', 'Playlists', 'Music Library'] },
    page: { module: '0.2.4' },
    status: {
      version: '0.7.93', arch: 'arm', binary: true, legacy: false, headless: false, running: true, timeout: 5,
      interactive: 'on', measured: true, diskFree: 900 * 1073741824, themeSize: '1280x720',
      activeTheme: '1280x720_theme', themeFiles: [], evo: healthyEvo(),
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
  broken.status.evo.buttons = false;
  broken.job = { kind: 'upgrade', name: 'Glass (file)', state: 'failed', error: { error: 'needs-evo', message: 'Glass 0.10.23 needs glass-evo 0.10.23 or later' }, endedAt: '2026-10-01T04:30:00Z' };
  broken.browse = { count: 0, names: [] };
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
  // "Something else" runs everything: the five common checks, and of glass-evo's what stands (the component paired, the display its own).
  assert.deepStrictEqual(kinds(r), ['cause:legacy', 'cause:binary', 'cause:disk', 'note:release', 'note:journal', 'ok:evo-component', 'ok:evo-display']);
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

test('the three system faults the Troubleshooting page knows are each named with their cure', () => {
  const find = function (graphics) {
    const f = healthy();
    f.screen = Object.assign({}, f.screen || {}, { graphics: graphics });
    return diagnose('screen', f).findings.find(function (x) { return x.check === 'graphics'; });
  };
  const x = { state: 'unknown', detail: '' };
  const agree = { versions: { 'libegl-mesa0': '24.2.8-1~bpo12+rpt5' }, agree: true, version: '24.2.8-1~bpo12+rpt5' };
  // A Pi 5 with a DSI panel and no vc4-kms-v3d: the panel's card alone, no render node.
  const noGpu = find({ asked: true, applies: true, ok: false, failed: 'card', driver: 'drm-rp1-dsi', connector: 'DSI-2', renderNode: false, cardError: 'permission', reason: '/dev/dri/card0 could not be opened: Permission denied (os error 13)', mesa: agree, x: x });
  assert.deepStrictEqual([noGpu.kind, noGpu.key, noGpu.with.driver, noGpu.go], ['cause', 'DIAG_GRAPHICS_NO_GPU', 'drm-rp1-dsi', 'screen']);
  assert.ok(strings.MANAGER_DIAG_GRAPHICS_NO_GPU.indexOf('vc4-kms-v3d-pi5') !== -1);
  // The GPU is there and the user's right to the card is not: a failed install took the render group.
  const noRight = find({ asked: true, applies: true, ok: false, failed: 'card', driver: 'vc4', connector: 'HDMI-A-1', renderNode: true, cardError: 'permission', reason: '/dev/dri/card1 could not be opened: Permission denied (os error 13)', mesa: agree, x: x });
  assert.deepStrictEqual([noRight.kind, noRight.key], ['cause', 'DIAG_GRAPHICS_NO_RIGHT']);
  assert.ok(strings.MANAGER_DIAG_GRAPHICS_NO_RIGHT.indexOf('render') !== -1);
  // Mesa out of step: libEGL_mesa will not load for want of its own libgallium.
  const mixed = { versions: { 'libegl-mesa0': '24.2.8-1~bpo12+rpt5', 'mesa-libgallium': '24.2.8-1~bpo12+rpt3' }, agree: false, version: '' };
  const mesa = find({ asked: true, applies: true, ok: false, failed: 'egl-vendor', driver: 'vc4', renderNode: true, cardError: '', reason: 'EGL gave no display for /dev/dri/card1; the EGL vendor library libEGL_mesa.so.0 would not load: libgallium-24.2.8-1~bpo12+rpt5.so: cannot open shared object file', mesa: mixed, x: x });
  assert.deepStrictEqual([mesa.kind, mesa.key], ['cause', 'DIAG_GRAPHICS_MESA']);
  assert.strictEqual(mesa.with.parts, 'libegl-mesa0 24.2.8-1~bpo12+rpt5, mesa-libgallium 24.2.8-1~bpo12+rpt3');
  assert.ok(strings.MANAGER_DIAG_GRAPHICS_MESA.indexOf('mesa-libgallium') !== -1);
  // The vendor library fails and Mesa agrees with itself: not that fault, the plain verdict.
  const other = find({ asked: true, applies: true, ok: false, failed: 'egl-vendor', driver: 'vc4', renderNode: true, cardError: '', reason: 'libEGL_mesa.so.0 would not load: libxshmfence.so.1: cannot open shared object file', mesa: agree, x: x });
  assert.strictEqual(other.key, 'DIAG_GRAPHICS_BROKEN');
  // A display before 0.10.1 says neither fact: the plain verdict, as before.
  const older = find({ asked: true, applies: true, ok: false, failed: 'card', driver: 'drm-rp1-dsi', renderNode: null, cardError: '', reason: '/dev/dri/card0 could not be opened: Permission denied (os error 13)', mesa: agree, x: x });
  assert.strictEqual(older.key, 'DIAG_GRAPHICS_BROKEN');
  // The strings exist in every language, with their holes.
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    assert.ok(s.MANAGER_DIAG_GRAPHICS_NO_GPU.indexOf('{driver}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_GRAPHICS_NO_RIGHT.indexOf('{reason}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_GRAPHICS_MESA.indexOf('{parts}') !== -1, lang);
  }
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

test('the panel switched off is named with its means, dark when nothing could switch it; a theme that leaves the screen bare names the backdrop switch', function () {
  let f = healthy();
  f.power = { display: true, power: 'on', by: 'backlight' };
  assert.strictEqual(one(diagnose('screen', f), 'panel-power'), undefined, 'on says nothing');
  f.power = { display: true, power: 'off', by: 'backlight' };
  let r = one(diagnose('screen', f), 'panel-power');
  assert.strictEqual(r.kind, 'cause');
  assert.strictEqual(r.key, 'DIAG_PANEL_OFF');
  assert.deepStrictEqual(r.with, { by: 'backlight' });
  assert.strictEqual(one(diagnose('no-meters', f), 'panel-power').kind, 'cause', 'no meters: the panel is off');
  f.power = { display: true, power: 'off', by: 'dark' };
  assert.strictEqual(one(diagnose('screen', f), 'panel-power').key, 'DIAG_PANEL_DARK');
  // On and dimmed while the player stands still: a note with the levels, under the screen, the meters and the face; set but not dimmed now, nothing.
  f.power = { display: true, power: 'on', by: 'backlight', maxBrightness: 255, idle: { on: true, level: 30, bright: 200, dimmed: true } };
  r = one(diagnose('screen', f), 'panel-power');
  assert.strictEqual(r.kind, 'note');
  assert.strictEqual(r.key, 'DIAG_PANEL_DIMMED');
  assert.deepStrictEqual(r.with, { level: 30, max: 255, bright: 200 });
  assert.strictEqual(r.go, 'screen');
  assert.strictEqual(one(diagnose('face', f), 'panel-power').key, 'DIAG_PANEL_DIMMED');
  f.power.idle.dimmed = false;
  assert.strictEqual(one(diagnose('screen', f), 'panel-power'), undefined, 'set and not dimmed now says nothing');
  f.power = { display: true, power: 'on', by: 'backlight', idle: { on: false, level: null, bright: null, dimmed: false } };
  assert.strictEqual(one(diagnose('screen', f), 'panel-power'), undefined, 'the switch off says nothing');
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    assert.ok(s.MANAGER_DIAG_PANEL_DIMMED.indexOf('{level}') !== -1 && s.MANAGER_DIAG_PANEL_DIMMED.indexOf('{bright}') !== -1, lang);
    for (const key of ['MANAGER_SCREEN_DIM', 'MANAGER_SCREEN_BRIGHTNESS_IDLE', 'MANAGER_SCREEN_DIM_HELP', 'MANAGER_SCREEN_DIM_SET', 'MANAGER_SCREEN_DIMMED_NOW']) assert.ok(s[key], lang + ' ' + key);
  }
  f.power = { display: false, power: 'off', by: '' };
  assert.strictEqual(one(diagnose('screen', f), 'panel-power'), undefined, 'no display on the screen: not the display\'s panel');
  // The backdrop: a smaller theme on a larger screen, the switch off, then on; a theme of the screen's size says nothing.
  f = healthy();
  f.display = { fit: false, backdrop: false, position: 'center', x: 0, y: 0 };
  f.status.themeSize = '1280x400';
  r = one(diagnose('artwork', f), 'backdrop');
  assert.strictEqual(r.kind, 'note');
  assert.deepStrictEqual(r.with, { theme: '1280x400', screen: '1280x720' });
  assert.strictEqual(r.go, 'artwork');
  f.display.backdrop = true;
  assert.strictEqual(one(diagnose('screen', f), 'backdrop').kind, 'ok');
  f.status.themeSize = '1280x720';
  assert.strictEqual(one(diagnose('screen', f), 'backdrop'), undefined);
  // Fitted: a 16:9 theme on a 16:9 screen leaves nothing bare; a strip does.
  f.display = { fit: true, backdrop: false };
  f.status.themeSize = '1920x1080';
  assert.strictEqual(one(diagnose('screen', f), 'backdrop'), undefined);
  f.status.themeSize = '1920x515';
  assert.strictEqual(one(diagnose('screen', f), 'backdrop').kind, 'note');
});

test('the buttons hidden by the look, an install refused for the pair or failed, a player with nothing to browse', () => {
  // In good order under the browse symptom: the buttons shown, the sources named, no install to speak of.
  const f = healthy();
  let r = diagnose('browse', f);
  assert.strictEqual(r.found, false, kinds(r).join(' '));
  assert.strictEqual(one(r, 'buttons').key, 'DIAG_BUTTONS_SHOWN');
  assert.deepStrictEqual(one(r, 'sources').with, { count: 3, names: 'Favourites, Playlists, Music Library' });
  assert.strictEqual(one(r, 'install-job'), undefined);
  assert.ok(one(r, 'evo-component') && one(r, 'channel') && one(r, 'evo-module'), 'the browser needs glass-evo, its module and the channel: ' + kinds(r).join(' '));
  // Hidden: the cause where touch or browsing is the complaint, worth knowing on the face, the look and the remotes; the Screen tab has the switch.
  f.status.evo.buttons = false;
  assert.strictEqual(one(diagnose('touch', f), 'buttons').kind, 'cause');
  assert.strictEqual(one(diagnose('browse', f), 'buttons').kind, 'cause');
  ['face', 'look', 'remotes'].forEach((s) => assert.strictEqual(one(diagnose(s, f), 'buttons').kind, 'note', s));
  assert.strictEqual(one(diagnose('touch', f), 'buttons').go, 'screen');
  assert.strictEqual(one(diagnose('touch', f), 'buttons').key, 'DIAG_BUTTONS_HIDDEN');
  // Shown, and leaving the idle screen too: checked, in its own words, with the setting named.
  const leaves = healthy(); leaves.status.evo.buttonsIdle = 'leave';
  assert.strictEqual(one(diagnose('touch', leaves), 'buttons').key, 'DIAG_BUTTONS_SHOWN_IDLE_LEAVES');
  assert.strictEqual(one(diagnose('touch', leaves), 'buttons').kind, 'ok');
  leaves.status.evo.buttons = false;
  assert.strictEqual(one(diagnose('touch', leaves), 'buttons').key, 'DIAG_BUTTONS_HIDDEN', 'hidden is hidden, whatever the idle word');
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    for (const key of ['MANAGER_DIAG_BUTTONS_SHOWN_IDLE_LEAVES', 'MANAGER_ST_EVO_BUTTONS_IDLE_LEAVE', 'MANAGER_LOOK_BUTTONS_IDLE', 'MANAGER_LOOK_BUTTONS_IDLE_STAY', 'MANAGER_LOOK_BUTTONS_IDLE_LEAVE', 'MANAGER_LOOK_BUTTONS_IDLE_HINT']) assert.ok(s[key], lang + ' ' + key);
  }
  // Without glass-evo there are no buttons to ask about; a sheet that says nothing of them neither.
  const bare = healthy(); bare.status.evo = null;
  assert.strictEqual(one(diagnose('touch', bare), 'buttons'), undefined);
  const unsaid = healthy(); delete unsaid.status.evo.buttons;
  assert.strictEqual(one(diagnose('touch', unsaid), 'buttons'), undefined);
  // The last install: refused for the pair is a cause under every symptom, with the way out; failed otherwise a note; one that went through, nothing.
  const refused = healthy();
  refused.job = { kind: 'upgrade', name: 'Glass (file)', state: 'failed', error: { error: 'needs-evo', message: 'Glass 0.10.23 needs glass-evo 0.10.23 or later: choose both zips and install them together' }, endedAt: '2026-10-01T04:30:00Z' };
  SYMPTOMS.forEach(function (s) {
    const x = one(diagnose(s, refused), 'install-job');
    assert.strictEqual(x.key, 'DIAG_INSTALL_NEEDS_PAIR', s);
    assert.strictEqual(x.kind, 'cause', s);
    assert.strictEqual(x.go, 'system', s);
    assert.strictEqual(diagnose(s, refused).found, true, s);
  });
  assert.deepStrictEqual(one(diagnose('other', refused), 'install-job').with, { name: 'Glass (file)', message: refused.job.error.message });
  refused.job = { kind: 'component', name: 'glass-evo (file)', state: 'failed', error: { error: 'needs-glass', message: 'glass-evo 0.10.23 needs Glass 0.10.23 or later: choose both zips and install them together' }, endedAt: '2026-10-01T04:30:00Z' };
  assert.strictEqual(one(diagnose('other', refused), 'install-job').key, 'DIAG_INSTALL_NEEDS_PAIR');
  refused.job = { kind: 'upgrade', name: 'Glass 0.10.24', state: 'failed', error: { error: 'network', message: 'ETIMEDOUT' }, endedAt: '2026-10-01T04:30:00Z' };
  const failed = one(diagnose('other', refused), 'install-job');
  assert.strictEqual(failed.key, 'DIAG_INSTALL_FAILED');
  assert.strictEqual(failed.kind, 'note');
  assert.strictEqual(failed.with.message, 'ETIMEDOUT');
  refused.job = { kind: 'component', name: 'glass-evo 0.10.23', state: 'done', error: null, endedAt: '2026-10-01T04:30:00Z' };
  assert.strictEqual(one(diagnose('other', refused), 'install-job'), undefined);
  // A zip kept for the other of the pair is worth knowing under every symptom, with what it waits for; nothing kept, nothing said.
  const kept = healthy();
  kept.held = { glass: { version: '0.10.33', needs: '0.10.33', at: '2026-10-17T15:00:00Z' }, evo: null };
  SYMPTOMS.forEach(function (s) {
    const x = one(diagnose(s, kept), 'held-zip');
    assert.strictEqual(x.key, 'DIAG_HELD_GLASS', s);
    assert.strictEqual(x.kind, 'note', s);
    assert.strictEqual(x.go, 'system', s);
  });
  assert.deepStrictEqual(one(diagnose('other', kept), 'held-zip').with, { version: '0.10.33', needs: '0.10.33' });
  kept.held = { glass: null, evo: { version: '0.10.33', needs: '0.10.33', at: '2026-10-17T15:00:00Z' } };
  assert.strictEqual(one(diagnose('other', kept), 'held-zip').key, 'DIAG_HELD_EVO');
  assert.strictEqual(one(diagnose('other', healthy()), 'held-zip'), undefined);
  const unsaidHeld = healthy(); delete unsaidHeld.held;
  assert.strictEqual(one(diagnose('other', unsaidHeld), 'held-zip'), undefined);
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    for (const key of ['MANAGER_DIAG_HELD_GLASS', 'MANAGER_DIAG_HELD_EVO']) assert.ok(s[key].indexOf('{version}') !== -1 && s[key].indexOf('{needs}') !== -1, lang + ' ' + key);
    for (const key of ['MANAGER_UPD_HELD_GLASS', 'MANAGER_UPD_HELD_EVO', 'MANAGER_ST_HELD_GLASS', 'MANAGER_ST_HELD_EVO']) assert.ok(s[key].indexOf('{version}') !== -1 && s[key].indexOf('{needs}') !== -1, lang + ' ' + key);
    for (const key of ['MANAGER_UPD_HELD_DISCARD', 'MANAGER_UPD_HELD_DISCARDED', 'MANAGER_ST_JOB_HELD']) assert.ok(s[key], lang + ' ' + key);
  }
  // Names that came with a source's markup, cleared: a note under browsing, the look and something else; none cleared, nothing said.
  const marked = healthy(); marked.markup = { count: 7, services: ['bandcamp', 'mixcloud'], at: '2026-10-24T10:00:00Z' };
  ['browse', 'look', 'other'].forEach(function (s) {
    const x = one(diagnose(s, marked), 'markup');
    assert.strictEqual(x.key, 'DIAG_MARKUP_CLEARED', s);
    assert.strictEqual(x.kind, 'note', s);
    assert.deepStrictEqual(x.with, { count: 7, services: 'bandcamp, mixcloud' });
  });
  assert.strictEqual(one(diagnose('touch', marked), 'markup'), undefined);
  const unmarked = healthy(); unmarked.markup = { count: 0, services: [], at: null };
  assert.strictEqual(one(diagnose('browse', unmarked), 'markup'), undefined);
  assert.strictEqual(one(diagnose('browse', healthy()), 'markup'), undefined);
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    assert.ok(s.MANAGER_DIAG_MARKUP_CLEARED.indexOf('{count}') !== -1 && s.MANAGER_DIAG_MARKUP_CLEARED.indexOf('{services}') !== -1, lang);
  }
  // Nothing to browse is the player's own: a cause with no tab to go to; sources that could not be read say nothing.
  const empty = healthy(); empty.browse = { count: 0, names: [] };
  const none = one(diagnose('browse', empty), 'sources');
  assert.strictEqual(none.key, 'DIAG_NO_SOURCES');
  assert.strictEqual(none.kind, 'cause');
  assert.strictEqual(none.go, null);
  const unread = healthy(); unread.browse = null;
  assert.strictEqual(one(diagnose('browse', unread), 'sources'), undefined);
  // The symptom and the rows have their words in every language, the findings with their holes.
  for (const lang of ['en', 'de', 'fr']) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    for (const key of ['MANAGER_DIAG_S_BROWSE', 'MANAGER_DIAG_BUTTONS_SHOWN', 'MANAGER_DIAG_BUTTONS_HIDDEN', 'MANAGER_DIAG_NO_SOURCES', 'MANAGER_ST_EVO_BUTTONS', 'MANAGER_ST_EVO_BUTTONS_SHOWN', 'MANAGER_ST_EVO_BUTTONS_HIDDEN', 'MANAGER_ST_LAST_JOB', 'MANAGER_ST_LAST_JOB_NONE', 'MANAGER_ST_JOB_DONE', 'MANAGER_ST_JOB_FAILED']) assert.ok(s[key], lang + ' ' + key);
    assert.ok(s.MANAGER_DIAG_INSTALL_NEEDS_PAIR.indexOf('{name}') !== -1 && s.MANAGER_DIAG_INSTALL_NEEDS_PAIR.indexOf('{message}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_INSTALL_FAILED.indexOf('{name}') !== -1 && s.MANAGER_DIAG_INSTALL_FAILED.indexOf('{message}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_SOURCES.indexOf('{count}') !== -1 && s.MANAGER_DIAG_SOURCES.indexOf('{names}') !== -1, lang);
  }
});
