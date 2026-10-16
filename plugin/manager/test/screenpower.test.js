'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { request, report, state, settings, wantedLevel, dimDelayMs } = require('../screenpower');

test('dim while nothing plays: the switch and the level when idle in a request, the settings, the level wanted, the moment', () => {
  assert.deepEqual(request({ dim: true }), { dim: true });
  assert.deepEqual(request({ dim: 'off' }), { dim: false });
  assert.deepEqual(request({ dim: '1', idleBrightness: '30' }), { dim: true, idleBrightness: 30 });
  assert.deepEqual(request({ dim: 'maybe' }), { error: 'bad-dim' });
  assert.deepEqual(request({ idleBrightness: -3 }), { error: 'bad-idle-brightness' });
  assert.deepEqual(request({ idleBrightness: 2.5 }), { error: 'bad-idle-brightness' });
  const config = (values) => ({ get: (k) => values[k] });
  // Off unless turned on; a level not set is null.
  assert.deepEqual(settings(config({})), { dim: false, idleLevel: null, level: null });
  assert.deepEqual(settings(config({ screenDimIdle: true, screenBrightnessIdle: 30, screenBrightness: '200' })), { dim: true, idleLevel: 30, level: 200 });
  assert.deepEqual(settings(config({ screenDimIdle: 'true', screenBrightnessIdle: 'x' })), { dim: true, idleLevel: null, level: null });
  // The level wanted: the idle one only while dimmed with the switch on and a level set.
  assert.equal(wantedLevel({ dim: true, idleLevel: 30, level: 200 }, true), 30);
  assert.equal(wantedLevel({ dim: true, idleLevel: 30, level: 200 }, false), 200);
  assert.equal(wantedLevel({ dim: false, idleLevel: 30, level: 200 }, true), 200, 'the switch off: the brightness at play');
  assert.equal(wantedLevel({ dim: true, idleLevel: null, level: 200 }, true), 200, 'no idle level: the brightness at play');
  assert.equal(wantedLevel({ dim: true, idleLevel: 30, level: null }, false), null);
  // The moment: when the clock comes, at once or after the persist period.
  assert.equal(dimDelayMs('none', 30), 0);
  assert.equal(dimDelayMs(undefined, 30), 0);
  assert.equal(dimDelayMs('persist', 30), 30000);
  assert.equal(dimDelayMs('persist', 'x'), 0);
  // The state carries the idle part, off and empty when nothing is set.
  assert.deepEqual(state(null, null, { running: true, ours: true }).idle, { on: false, level: null, bright: null, dimmed: false });
  assert.deepEqual(state(null, null, { running: true, ours: true }, { dim: true, idleLevel: 30, level: 200, dimmed: true }).idle, { on: true, level: 30, bright: 200, dimmed: true });
});

test('a request names the power on, off or toggle, a brightness as a whole number, or nothing', () => {
  assert.deepEqual(request({ power: 'off' }), { power: 'off' });
  assert.deepEqual(request({ power: 'ON' }), { power: 'on' });
  assert.deepEqual(request({ power: 'toggle' }), { power: 'toggle' });
  assert.deepEqual(request({ power: true }), { power: 'on' });
  assert.deepEqual(request({ power: '0' }), { power: 'off' });
  assert.deepEqual(request({ brightness: 120 }), { brightness: 120 });
  assert.deepEqual(request({ brightness: '80' }), { brightness: 80 });
  assert.deepEqual(request({}), {});
  assert.deepEqual(request(null), {});
  assert.deepEqual(request({ power: 'dim' }), { error: 'bad-power' });
  assert.deepEqual(request({ brightness: -1 }), { error: 'bad-brightness' });
  assert.deepEqual(request({ brightness: 1.5 }), { error: 'bad-brightness' });
  assert.deepEqual(request({ brightness: 'bright' }), { error: 'bad-brightness' });
});

test("the display's report is kept as it came, the backlight only with a top", () => {
  const r = report({ kind: 'screen', power: 'off', by: 'backlight', brightness: 120, max: 255, ours: true });
  assert.equal(r.power, 'off');
  assert.equal(r.by, 'backlight');
  assert.equal(r.brightness, 120);
  assert.equal(r.maxBrightness, 255);
  assert.equal(r.ours, true);
  const bare = report({ kind: 'screen', power: 'on', by: 'dpms' });
  assert.equal(bare.power, 'on');
  assert.equal(bare.by, 'dpms');
  assert.equal(bare.brightness, null);
  assert.equal(bare.ours, false);
  assert.equal(report({ by: 'magic' }).by, '');
  assert.equal(report({ power: 'nonsense' }).power, 'on');
});

test('the state answers from the report, else from the wish, and says whether a display is there', () => {
  const none = state(null, null, { running: false, ours: false });
  assert.deepEqual(none, { power: 'on', by: '', brightness: null, maxBrightness: null, backlight: false, ours: false, display: false, reported: false, idle: { on: false, level: null, bright: null, dimmed: false } });
  const wished = state(null, { power: 'off' }, { running: true, ours: true });
  assert.equal(wished.power, 'off');
  assert.equal(wished.ours, true);
  assert.equal(wished.display, true);
  assert.equal(wished.reported, false);
  const told = state(report({ power: 'off', by: 'backlight', brightness: 50, max: 255, ours: true }), null, { running: true, ours: false });
  assert.equal(told.power, 'off');
  assert.equal(told.by, 'backlight');
  assert.equal(told.backlight, true);
  assert.equal(told.brightness, 50);
  assert.equal(told.ours, true, "the display's word on the screen beats the plugin's guess");
  assert.equal(told.reported, true);
});
