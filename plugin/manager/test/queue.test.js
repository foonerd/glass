'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { compact } = require('../queue');

test('a queue keeps the fields a display reads, the name standing in for a title, the location and the cover for a changer', () => {
  const items = compact([
    { name: 'One', artist: ' A ', album: 'X', duration: 12.5, uri: 'mnt/x', albumart: '/a' },
    { title: 'Two', name: 'not this', duration: '7' },
    { name: 'Three', duration: -1 },
    null
  ]);
  assert.deepEqual(items, [
    { title: 'One', artist: 'A', album: 'X', duration: 12.5, uri: 'mnt/x', albumart: '/a' },
    { title: 'Two', artist: '', album: '', duration: 7, uri: '', albumart: '' },
    { title: 'Three', artist: '', album: '', duration: 0, uri: '', albumart: '' },
    { title: '', artist: '', album: '', duration: 0, uri: '', albumart: '' }
  ]);
  assert.deepEqual(compact(undefined), []);
  assert.deepEqual(compact({ queue: [] }), []);
});
