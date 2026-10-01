'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { settingsOf, plan, resetPlan, frostSuits, themeKeys } = require('../facelook');

const current = { 'screen.rotation': '270', 'face.size': 'car', 'face.frost.suits': 'true', 'face.theme': 'Midnight', 'face.colours.accent': '#ff8800', 'face.glass.bar': '0.6', 'face.': 'x' };

test('the face\'s settings are the face. keys by name', function () {
  assert.deepStrictEqual(settingsOf(current), { size: 'car', 'frost.suits': 'true', theme: 'Midnight', 'colours.accent': '#ff8800', 'glass.bar': '0.6' });
  assert.deepStrictEqual(settingsOf(null), {});
});

test('a request becomes the keys that change, and only those', function () {
  assert.deepStrictEqual(plan(current, { 'colours.accent': '#ff8800', 'glass.bar': 0.8, 'colours.tint': ' #101820 ', 'clock.ink': null, theme: '' }).changes,
    { 'face.glass.bar': '0.8', 'face.colours.tint': '#101820', 'face.theme': null });
  assert.deepStrictEqual(plan(current, {}).changes, {});
  assert.deepStrictEqual(plan({}, { 'glass.frost': 'on', theme: 'Warm Glow' }).changes, { 'face.glass.frost': 'on', 'face.theme': 'Warm Glow' });
});

test('a name or a value that is not one is refused, and so are the keys set elsewhere', function () {
  assert.strictEqual(plan(current, { 'Glass.Bar': '1' }).error, 'bad-name');
  assert.strictEqual(plan(current, { 'a.b.c.d.e': '1' }).error, 'bad-name');
  assert.strictEqual(plan(current, { size: 'car' }).error, 'bad-name', 'the size has its own route');
  assert.strictEqual(plan(current, { 'frost.suits': 'true' }).error, 'bad-name', 'the board\'s word is the plugin\'s');
  assert.strictEqual(plan(current, { 'glass.bar': '0.5\n[current]' }).error, 'bad-value');
  assert.strictEqual(plan(current, { 'glass.bar': 'a=b' }).error, 'bad-value');
  assert.strictEqual(plan(current, { 'glass.bar': 'x'.repeat(65) }).error, 'bad-value');
  assert.strictEqual(plan(current, null).error, 'bad-request');
  assert.strictEqual(plan(current, ['x']).error, 'bad-request');
});

test('a reset removes the look and keeps the size and the board\'s word', function () {
  assert.deepStrictEqual(resetPlan(current).changes, { 'face.theme': null, 'face.colours.accent': null, 'face.glass.bar': null });
  assert.deepStrictEqual(resetPlan({ 'face.size': 'car' }).changes, {});
});

test('a pattern for a time or a date is a value; what would break a line is not', function () {
  assert.deepStrictEqual(plan({}, { 'clock.format': '%-I:%M %p', 'date.format': '%A, %B %-d (%Y)', 'date.place': 'top' }).changes,
    { 'face.clock.format': '%-I:%M %p', 'face.date.format': '%A, %B %-d (%Y)', 'face.date.place': 'top' });
  assert.strictEqual(plan({}, { 'date.format': '%d/%m/%Y' }).changes['face.date.format'], '%d/%m/%Y');
  ['%H=%M', '"%H:%M"', '%H;%M', "%H'%M", '[%H]'].forEach(function (bad) {
    assert.strictEqual(plan({}, { 'clock.format': bad }).error, 'bad-value', bad);
  });
});

test('a face theme\'s text is its keys under their sections', function () {
  assert.deepStrictEqual(themeKeys('# a theme\n[Theme]\nname = Midnight\n\n[clock]\nformat = %-I:%M %p\n ; note\nshow=off\n[date]\nplace = top\nnot a pair\n = lost\n'),
    { 'theme.name': 'Midnight', 'clock.format': '%-I:%M %p', 'clock.show': 'off', 'date.place': 'top' });
  assert.deepStrictEqual(themeKeys('glass.bar = 0.5'), { 'glass.bar': '0.5' });
  assert.deepStrictEqual(themeKeys(null), {});
});

test('frost suits the boards with room for it', function () {
  ['pi5', 'pi4', 'x64'].forEach((c) => assert.ok(frostSuits(c), c));
  ['pi3', 'zero2', 'pi1', 'pi', 'other', 'other4', undefined].forEach((c) => assert.ok(!frostSuits(c), String(c)));
});
