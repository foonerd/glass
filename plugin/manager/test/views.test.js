'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const views = require('../views');

test('the views carry the face where the user says and a module is here', () => {
  assert.equal(views.modeOf(undefined), 'follow');
  assert.equal(views.modeOf(' Face '), 'face');
  assert.equal(views.modeOf('THEME'), 'theme');
  assert.equal(views.modeOf('always'), 'follow', 'anything else follows the screen');
  // Following the screen: the face where glass-evo holds it.
  assert.equal(views.carriesFace('follow', 'glass-evo', true), true);
  assert.equal(views.carriesFace('follow', 'kiosk', true), false);
  // The Glass interface wherever the component is here, whoever holds the screen.
  assert.equal(views.carriesFace('face', 'kiosk', true), true);
  // The theme alone, whoever holds the screen.
  assert.equal(views.carriesFace('theme', 'glass-evo', true), false);
  // No module, no face, whatever is asked.
  for (const mode of views.MODES) {
    assert.equal(views.carriesFace(mode, 'glass-evo', false), false);
  }
});

test('a face theme is named as the face names one', () => {
  assert.equal(views.themeName(' Warm '), 'Warm');
  assert.equal(views.themeName('Night Drive'), 'Night Drive');
  for (const bad of ['', '.hidden', '../up', 'a/b', 'a\\b', null, undefined]) {
    assert.equal(views.themeName(bad), null);
  }
});
