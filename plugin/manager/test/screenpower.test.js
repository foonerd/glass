'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { request, report, state } = require('../screenpower');

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
  assert.deepEqual(none, { power: 'on', by: '', brightness: null, maxBrightness: null, backlight: false, ours: false, display: false, reported: false });
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
