'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { compact, hasCover } = require('../queue');

test('a cover is known when an item of the queue reported it, and for no other address', () => {
  const items = compact([
    { name: 'One', albumart: 'https://i.example/one.jpg' },
    { name: 'Two', albumart: '/albumart?path=two' }
  ]);
  assert.equal(hasCover(items, 'https://i.example/one.jpg'), true);
  assert.equal(hasCover(items, '/albumart?path=two'), true);
  assert.equal(hasCover(items, 'https://i.example/three.jpg'), false);
  assert.equal(hasCover(items, ''), false);
  assert.equal(hasCover(null, 'https://i.example/one.jpg'), false);
});

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
