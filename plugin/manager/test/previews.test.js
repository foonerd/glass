'use strict';
// What a preview render produced is read from the pictures it wrote, not
// from the display's log, which the player's log level may hold back.
const test = require('node:test');
const assert = require('node:assert/strict');
const { metersProduced, sectionNames, renderEnv } = require('../previews');

const files = ['Bars.png', 'Bars.thumb.png', 'VU.png', 'VU.thumb.png', 'Snow.png', 'Snow.thumb.png', '.stamp.json', 'notes.txt'];

test('with a silent log the pictures on disk are found, in the theme\'s order', () => {
  assert.deepEqual(metersProduced(files, '', ['VU', 'Bars', 'Snow']), ['VU', 'Bars', 'Snow']);
  assert.deepEqual(metersProduced(files, undefined, []), ['Bars', 'Snow', 'VU'], 'by name when the theme gives no order');
});

test('the display\'s own order wins where its log says one', () => {
  const log = 'glass: frame.rate=30\nglass: snapshot /w/t/Snow.png\nglass: snapshot /w/t/Snow.thumb.png\nglass: snapshot /w/t/VU.png\n';
  assert.deepEqual(metersProduced(files, log, ['VU', 'Bars', 'Snow']), ['Snow', 'VU', 'Bars']);
});

test('a line naming a picture that is not there counts for nothing, and thumbnails are no meters', () => {
  assert.deepEqual(metersProduced(['VU.png', 'VU.thumb.png'], 'glass: snapshot /w/t/Gone.png\n', ['Gone', 'VU']), ['VU']);
  assert.deepEqual(metersProduced([], 'glass: snapshot /w/t/VU.png\n', ['VU']), []);
  assert.deepEqual(metersProduced(undefined, undefined, undefined), []);
});

test('the section names of a meters file come in its order', () => {
  assert.deepEqual(sectionNames('[PLX-500 VU]\nmeter.type = linear\n\n [PLX-500 Bars] \nx = 1\n[x]\n'), ['PLX-500 VU', 'PLX-500 Bars', 'x']);
  assert.deepEqual(sectionNames(''), []);
});

test('a preview is rendered by Glass\'s own display, with no screen and no face', () => {
  const display = { DISPLAY: ':0', GLASS_HOME: '/plugin', GLASS_BIN: '/data/glass-evo/bin/glass-evo', GLASS_SCREEN_OURS: '1', GLASS_FACES: '/faces' };
  const env = renderEnv(display);
  assert.equal(env.GLASS_BIN, undefined, 'never the face\'s binary');
  assert.equal(env.GLASS_SCREEN_OURS, undefined, 'never a screen of its own');
  assert.equal(env.DISPLAY, undefined, 'no screen to open');
  assert.equal(env.GLASS_HOME, '/plugin', 'the rest as the display has it');
  assert.equal(display.GLASS_BIN, '/data/glass-evo/bin/glass-evo', 'the display\'s own environment is left as it was');
});
