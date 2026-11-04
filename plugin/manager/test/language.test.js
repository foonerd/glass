'use strict';

const test = require('node:test');
const assert = require('node:assert');
const path = require('path');
const language = require('../language');

const SHIPPED = language.shipped(path.join(__dirname, '..', '..', 'i18n'));

test('the shipped languages are read from the strings folder', () => {
  assert.deepStrictEqual(SHIPPED, ['de', 'en', 'es', 'fr', 'it', 'ja', 'nl', 'pl', 'pt', 'ru', 'zh']);
  assert.deepStrictEqual(language.shipped('/nowhere/at/all'), []);
});

test('a language set in Glass stands over the player\'s', () => {
  assert.strictEqual(language.pick('de', 'fr', SHIPPED), 'de');
  assert.strictEqual(language.pick(' PL ', 'fr', SHIPPED), 'pl', 'as written anywhere');
  assert.strictEqual(language.pick('', 'fr', SHIPPED), 'fr', 'nothing set: the player\'s');
  assert.strictEqual(language.pick(undefined, undefined, SHIPPED), 'en', 'nothing at all: English');
});

test('a language Glass does not ship is passed over', () => {
  assert.strictEqual(language.resolve('sv', SHIPPED), '');
  assert.strictEqual(language.pick('sv', 'de', SHIPPED), 'de', 'an unshipped setting leaves the player\'s word');
  assert.strictEqual(language.pick('', 'sv', SHIPPED), 'en', 'an unshipped player language leaves English');
});

test('a code with a region falls to its language', () => {
  assert.strictEqual(language.resolve('pt_BR', SHIPPED), 'pt');
  assert.strictEqual(language.resolve('zh-CN', SHIPPED), 'zh');
  assert.strictEqual(language.pick('', 'pt_BR', SHIPPED), 'pt');
  assert.strictEqual(language.clean('<script>'), 'script', 'letters alone survive');
});
