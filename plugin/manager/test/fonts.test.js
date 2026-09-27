'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { fontKind, safeFontName } = require('../fonts');

test('a font file is known by its first bytes', () => {
  assert.strictEqual(fontKind(Buffer.from([0, 1, 0, 0, 0, 12])), 'ttf');
  assert.strictEqual(fontKind(Buffer.from('true0000')), 'ttf');
  assert.strictEqual(fontKind(Buffer.from('OTTO0000')), 'otf');
  assert.strictEqual(fontKind(Buffer.from('ttcf0000')), null, 'a collection is refused');
  assert.strictEqual(fontKind(Buffer.from('PK\u0003\u0004')), null, 'a zip is not a font');
  assert.strictEqual(fontKind(Buffer.from([0, 1])), null);
});

test('an uploaded font keeps a plain name with the extension its bytes say', () => {
  assert.strictEqual(safeFontName('My Font-Bold.TTF', 'ttf'), 'My Font-Bold.ttf');
  assert.strictEqual(safeFontName('C:\\fonts\\Sans.otf', 'otf'), 'Sans.otf');
  assert.strictEqual(safeFontName('../../etc/passwd', 'ttf'), 'passwd.ttf');
  assert.strictEqual(safeFontName('.hidden', 'ttf'), 'hidden.ttf');
  assert.strictEqual(safeFontName('###', 'ttf'), '___.ttf');
  assert.strictEqual(safeFontName('', 'ttf'), null);
});
