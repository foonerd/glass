'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { wanted } = require('../themefolder');

test('the chosen theme is put back when the configuration names another folder, as the template does after an update', () => {
  assert.equal(wanted('800x480', '1280x720_glass_looks'), true, 'the template folder against the chosen one');
  assert.equal(wanted('', '1280x720_glass_looks'), true, 'a configuration naming nothing');
  assert.equal(wanted(undefined, '1280x400'), true, 'a chosen folder without an underscore is a theme too');
  assert.equal(wanted(' 1280x720_glass_looks ', '1280x720_glass_looks'), false, 'the same folder, whitespace aside');
});

test('nothing is put back without a chosen theme, or with one that is not a plain name', () => {
  assert.equal(wanted('800x480', ''), false);
  assert.equal(wanted('800x480', undefined), false);
  assert.equal(wanted('800x480', 'a/b'), false);
  assert.equal(wanted('800x480', '../x'), false);
});
