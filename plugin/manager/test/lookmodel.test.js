'use strict';

const test = require('node:test');
const assert = require('node:assert');
const look = require('../lookmodel');

const DARK = { 'theme.name': 'Dark Glass', 'theme.description': 'Black, fairly solid', 'colours.tint': '#000000', 'glass.bar': '0.85', 'glass.sheet': '0.88', 'clock.glass': '0.8', 'date.glass': '0.8' };
const CLEAR = { 'glass.bar': '0.1', 'glass.sheet': '0.6', 'glass.hairline': '0', 'glass.frost': 'off', 'clock.glass': '0', 'date.glass': '0' };

test('a look is laid over the built-in one, and the adjustments over the look', () => {
  const builtIn = look.lay();
  assert.deepStrictEqual(builtIn, look.BUILTIN);
  assert.notStrictEqual(builtIn, look.BUILTIN, 'a copy, for the page to change');
  const dark = look.lay({}, DARK);
  assert.strictEqual(dark['colours.tint'], '#000000');
  assert.strictEqual(dark['colours.accent'], 'artwork', 'what the look leaves out is the built-in');
  assert.strictEqual(dark['theme.name'], undefined, 'a name is not a setting');
  const mine = look.lay({}, DARK, { 'clock.format': '%H:%M:%S', size: 'car', 'frost.suits': 'true' });
  assert.strictEqual(mine['clock.format'], '%H:%M:%S');
  assert.strictEqual(mine['glass.bar'], '0.85');
  assert.strictEqual(Object.keys(mine).length, look.KEYS.length, 'nothing the panel does not set');
  // A component's own word on the built-in look stands before the panel's.
  assert.strictEqual(look.lay({ 'glass.bar': '0.7' })['glass.bar'], '0.7');
  // The other spelling, and a key with nothing after it.
  assert.strictEqual(look.lay({ 'colors.tint': '#102030', 'colours.ink': ' ' })['colours.tint'], '#102030');
  assert.strictEqual(look.lay({ 'colours.ink': ' ' })['colours.ink'], '#f2f2f5');
});

test('two values are the same where the face reads them the same', () => {
  assert.ok(look.same('glass.bar', '0.75', '0.750'));
  assert.ok(look.same('buttons.opacity', '1.0', '1'));
  assert.ok(look.same('measure.bar', '72', '72.0'));
  assert.ok(!look.same('glass.bar', '0.75', '0.76'));
  assert.ok(look.same('colours.ink', '#F2F2F5', '#f2f2f5'));
  assert.ok(look.same('glass.frost', 'Auto', 'auto'));
  assert.ok(!look.same('clock.format', '%H:%M %p', '%H:%M %P'), 'a pattern is taken to the letter');
  assert.ok(!look.same('colours.tint', 'artwork', '#000000'));
});

test('a save says what differs from the look and removes the rest', () => {
  const base = look.lay({}, DARK);
  const values = look.lay({}, DARK, { 'clock.format': '%-I:%M %p', 'glass.bar': '0.850', 'colours.tint': 'artwork' });
  assert.ok(look.differs(base, values));
  const set = look.changes(base, values);
  assert.deepStrictEqual(Object.keys(set), look.KEYS, 'every key is named, so one set before goes');
  assert.strictEqual(set['clock.format'], '%-I:%M %p');
  assert.strictEqual(set['colours.tint'], 'artwork', 'the built-in word, said over a look that fixed the colour');
  assert.strictEqual(set['glass.bar'], null, 'the look\'s own value is not written among the settings');
  assert.strictEqual(Object.keys(set).filter((key) => set[key] !== null).length, 2);
  assert.ok(!look.differs(base, look.lay({}, DARK)));
  assert.ok(Object.keys(look.changes(base, base)).every((key) => look.changes(base, base)[key] === null));
});

test('frost is the user\'s or the look\'s word, else as suits the board', () => {
  assert.strictEqual(look.frostOn(look.lay(), true), true);
  assert.strictEqual(look.frostOn(look.lay(), false), false);
  assert.strictEqual(look.frostOn(look.lay({}, CLEAR), true), false);
  assert.strictEqual(look.frostOn(look.lay({}, { 'glass.frost': 'on' }), false), true);
});

test('one slider moves every background, each at its distance, and one that is off stays off', () => {
  const base = look.lay();
  assert.deepStrictEqual(look.backgrounds(base, base, 0.6), { 'glass.bar': '0.60', 'glass.sheet': '0.63', 'clock.glass': '0.40', 'date.glass': '0.40' });
  assert.deepStrictEqual(look.backgrounds(base, base, 1), { 'glass.bar': '1.00', 'glass.sheet': '1.00', 'clock.glass': '0.80', 'date.glass': '0.80' });
  assert.deepStrictEqual(look.backgrounds(base, base, 0.02), { 'glass.bar': '0.10', 'glass.sheet': '0.13', 'clock.glass': '0.10', 'date.glass': '0.10' }, 'never less than the face draws');
  const dark = look.lay({}, DARK);
  assert.strictEqual(look.backgrounds(dark, dark, 0.5)['clock.glass'], '0.45', 'the look\'s own distance');
  const clear = look.lay({}, CLEAR);
  assert.deepStrictEqual(look.backgrounds(clear, clear, 0.5), { 'glass.bar': '0.50', 'glass.sheet': '1.00' }, 'no background is not made one');
  // The clock's background turned off by the user stays off; the date's follows.
  const mine = look.lay({}, {}, { 'clock.glass': '0' });
  assert.deepStrictEqual(Object.keys(look.backgrounds(base, mine, 0.6)), ['glass.bar', 'glass.sheet', 'date.glass']);
});

test('the words for how solid and how large', () => {
  assert.deepStrictEqual(['0', '0.1', '0.3', '0.75', '0.85', '0.95'].map(look.solidWord), ['NONE', 'FAINT', 'LIGHT', 'FAIR', 'FAIR', 'SOLID']);
  assert.strictEqual(look.sizeOf('measure.clock', '144'), 100);
  assert.strictEqual(look.sizeOf('measure.clock', '180'), 125);
  assert.strictEqual(look.sizeOf('measure.bar', 'tall'), 100);
  assert.deepStrictEqual(['36', '72', '100', '144'].map((v) => look.sizeWord('measure.bar', v)), ['SMALL', 'STANDARD', 'LARGE', 'HUGE']);
  assert.strictEqual(look.unitsOf('measure.date', 150), '60');
  assert.strictEqual(look.sizeOf('measure.date', look.unitsOf('measure.date', 85)), 85);
});

test('colours and switches as the face reads them', () => {
  assert.strictEqual(look.colourOf('#FFB347'), '#ffb347');
  assert.strictEqual(look.colourOf('artwork'), null);
  assert.strictEqual(look.colourOf('#fff'), null);
  assert.strictEqual(look.rgb('#ffb347'), '255,179,71');
  assert.ok(look.isOn('On') && look.isOn('true') && !look.isOn('off') && !look.isOn('auto'));
});

test('a pattern shows as the face\'s clock would show it', () => {
  const at = look.sample();
  const shown = (pattern) => look.strftime(pattern, at);
  assert.deepStrictEqual(look.CLOCK_PATTERNS.map(shown), ['13:05', '13:05:09', '1:05 PM', '1:05:09 PM']);
  assert.deepStrictEqual(look.DATE_PATTERNS.map(shown), ['Thursday 1 October', 'Thursday, October 1', 'Thu 1 Oct 2026', '01/10/2026', '10/01/2026', '01.10.2026', '2026-10-01']);
  assert.strictEqual(shown('%I:%M %P'), '01:05 pm');
  assert.strictEqual(shown('%l:%M|%k|%e|%_d|%-m|%0e'), ' 1:05|13| 1| 1|10|01');
  assert.strictEqual(shown('%R %T %D %F'), '13:05 13:05:09 10/01/26 2026-10-01');
  assert.strictEqual(shown('%^a %^B %j %y %h'), 'THU OCTOBER 274 26 Oct');
  assert.strictEqual(shown('100%% at %H, %Q and %'), '100% at 13, %Q and %', 'what it does not know stands as written');
  assert.strictEqual(look.strftime('%-I %p', new Date(2026, 0, 1, 0, 30)), '12 AM');
  assert.strictEqual(look.strftime('%-I %p %j', new Date(2026, 11, 31, 12, 0)), '12 PM 365');
});

test('a line is set no larger than the face would set it', () => {
  // A 1280 by 720 picture, the unit one pixel, the bar 72 high.
  const most = look.idleMost(1280, 720, 1, 72, 0, 'top');
  assert.deepStrictEqual(most.date, [1184, 180]);
  assert.deepStrictEqual(most.clock, [1184, 360], 'half the picture at most');
  assert.strictEqual(look.fitted(144, [600, 170], most.clock), 144, 'what fits stays as wanted');
  assert.strictEqual(look.fitted(300, [1480, 350], most.clock), 240, 'too wide: smaller by as much');
  assert.strictEqual(look.fitted(400, [1000, 480], most.clock), 300, 'too tall: smaller by as much');
  // A date at the top takes its glass and two margins; beside the clock, the gap.
  assert.strictEqual(look.idleMost(1280, 720, 1, 300, 40, 'top').clock[1], 720 - 300 - (40 + 64) - 64);
  assert.strictEqual(look.idleMost(1280, 720, 1, 300, 40, 'above').clock[1], 720 - 300 - 48 - 64);
  // A larger face: every measure grows with the unit.
  assert.deepStrictEqual(look.idleMost(1280, 720, 2, 144, 0, 'top').clock, [1088, 360]);
});
