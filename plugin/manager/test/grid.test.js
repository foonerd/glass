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

test('an element larger than its cells is set against the far side, so that "top" moves it up', function () {
  assert.strictEqual(LOOK.turned('top', false), 'top', 'one that fits stands against the side named');
  assert.strictEqual(LOOK.turned('top', true), 'bottom', 'one that runs over stands against the far side');
  assert.strictEqual(LOOK.turned('left', true), 'right');
  assert.strictEqual(LOOK.turned('right', true), 'left');
  assert.strictEqual(LOOK.turned('bottom', true), 'top');
  assert.strictEqual(LOOK.turned('centre', true), 'centre', 'the middle stays the middle');
  assert.strictEqual(LOOK.turned('middle', true), 'middle');
  assert.strictEqual(LOOK.flexed('left'), 'flex-start');
  assert.strictEqual(LOOK.flexed('bottom'), 'flex-end');
  assert.strictEqual(LOOK.flexed('centre'), 'center');
  assert.strictEqual(LOOK.flexed(LOOK.turned('top', true)), 'flex-end', 'a large clock aligned top sits against the foot of its cells');
});

test('the built-in look knows the clock\'s grid keys, with the clock off the grid', function () {
  assert.strictEqual(LOOK.BUILTIN['clock.place'], '');
  assert.strictEqual(LOOK.BUILTIN['clock.align'], 'centre middle');
  assert.strictEqual(LOOK.BUILTIN['date.align'], 'centre middle');
  assert.strictEqual(LOOK.BUILTIN['date.margin'], '20');
});
