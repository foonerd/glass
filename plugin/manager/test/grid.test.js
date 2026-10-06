'use strict';
const test = require('node:test');
const assert = require('node:assert');
const LOOK = require('../lookmodel');

test('a place names cells of the grid, rows then columns, each a name or a range', function () {
  assert.deepStrictEqual(LOOK.cells('middle right'), { r0: 1, r1: 1, c0: 2, c1: 2 });
  assert.deepStrictEqual(LOOK.cells('middle left-right'), { r0: 1, r1: 1, c0: 0, c1: 2 }, 'a row');
  assert.deepStrictEqual(LOOK.cells('Middle-Bottom center-right'), { r0: 1, r1: 2, c0: 1, c1: 2 }, 'four cells, either spelling');
  assert.deepStrictEqual(LOOK.cells('bottom-top right-left'), { r0: 0, r1: 2, c0: 0, c1: 2 }, 'either way round');
  ['', 'top', 'middle left right', 'upper left', 'top-low left', undefined, null].forEach(function (none) {
    assert.strictEqual(LOOK.cells(none), null, JSON.stringify(none) + ' names no cells');
  });
  assert.strictEqual(LOOK.cellsText({ r0: 1, r1: 2, c0: 1, c1: 2 }), 'middle-bottom centre-right');
  assert.strictEqual(LOOK.cellsText(LOOK.cells('middle left-right')), 'middle left-right', 'there and back');
});

test('an alignment is across and down in either order, the middle unless said', function () {
  assert.deepStrictEqual(LOOK.align(''), { across: 'centre', down: 'middle' });
  assert.deepStrictEqual(LOOK.align('bottom right'), { across: 'right', down: 'bottom' });
  assert.deepStrictEqual(LOOK.align('left'), { across: 'left', down: 'middle' });
  assert.strictEqual(LOOK.align('leftish'), null);
});

test('the built-in look knows the clock\'s grid keys, with the clock off the grid', function () {
  assert.strictEqual(LOOK.BUILTIN['clock.place'], '');
  assert.strictEqual(LOOK.BUILTIN['clock.align'], 'centre middle');
});
