'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { productOf, standing } = require('../remotebehind');

test('a remote is its face\'s product, else Glass at its release', () => {
  assert.deepStrictEqual(productOf({ release: '0.8.54', face: 'glass-evo 0.1.28' }), { product: 'glass-evo', version: '0.1.28' });
  assert.deepStrictEqual(productOf({ release: '0.8.54', face: '' }), { product: 'Glass', version: '0.8.54' });
  assert.deepStrictEqual(productOf({ release: '0.8.54' }), { product: 'Glass', version: '0.8.54' });
  assert.deepStrictEqual(productOf({ release: 'dev', face: 'a face' }), { product: 'Glass', version: null }, 'a face with no version is not a product');
  assert.deepStrictEqual(productOf(null), { product: 'Glass', version: null });
});

test('a remote is behind where a later release of what it is was seen, and not a test release', () => {
  const latest = { Glass: { version: '0.8.57', prerelease: false }, 'glass-evo': { version: '0.1.28', prerelease: false } };
  assert.deepStrictEqual(standing({ release: '0.8.54' }, latest), { product: 'Glass', version: '0.8.54', latest: '0.8.57', behind: true });
  assert.deepStrictEqual(standing({ release: '0.8.57' }, latest), { product: 'Glass', version: '0.8.57', latest: '0.8.57', behind: false });
  assert.strictEqual(standing({ release: '0.8.58' }, latest).behind, false, 'ahead is not behind');
  // The bundle follows glass-evo's releases, whatever Glass it is built on.
  assert.deepStrictEqual(standing({ release: '0.8.54', face: 'glass-evo 0.1.28' }, latest), { product: 'glass-evo', version: '0.1.28', latest: '0.1.28', behind: false });
  assert.strictEqual(standing({ release: '0.8.54', face: 'glass-evo 0.1.20' }, latest).behind, true);
  // Nothing can be said: a test release seen, none seen, a remote without a version, another product.
  assert.strictEqual(standing({ release: '0.8.54' }, { Glass: { version: '0.8.58', prerelease: true } }).behind, null);
  assert.strictEqual(standing({ release: '0.8.54' }, {}).behind, null);
  assert.strictEqual(standing({ release: '0.8.54' }, null).behind, null);
  assert.strictEqual(standing({ release: '' }, latest).behind, null);
  assert.strictEqual(standing({ release: '0.8.54', face: 'other-face 1.0.0' }, latest).behind, null);
});
