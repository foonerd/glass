'use strict';

const test = require('node:test');
const assert = require('node:assert');
const backgrounds = require('../backgrounds');
const facelook = require('../facelook');

test('a picture is one by its first bytes, and takes the ending of what it is', () => {
  assert.strictEqual(backgrounds.endingOf(Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0])), '.jpg');
  assert.strictEqual(backgrounds.endingOf(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0])), '.png');
  assert.strictEqual(backgrounds.endingOf(Buffer.from('RIFF\0\0\0\0WEBP', 'latin1')), '.webp');
  assert.strictEqual(backgrounds.endingOf(Buffer.from('GIF89a\0\0\0\0\0\0', 'latin1')), null, 'a GIF is not taken');
  assert.strictEqual(backgrounds.endingOf(Buffer.from('#!/bin/sh\n\n\n')), null);
});

test('an uploaded picture is kept under a name the settings can carry', () => {
  assert.strictEqual(backgrounds.safeName('Storm over the lake.JPEG', '.jpg'), 'Storm over the lake.jpg');
  assert.strictEqual(backgrounds.safeName('C:\\Users\\me\\Pictures\\sea (2).png', '.png'), 'sea (2).png');
  assert.strictEqual(backgrounds.safeName('../../etc/passwd', '.jpg'), 'passwd.jpg');
  assert.strictEqual(backgrounds.safeName('été à la mer.webp', '.webp'), 't- - la mer.webp');
  assert.strictEqual(backgrounds.safeName('', '.png'), 'picture.png');
  assert.strictEqual(backgrounds.safeName('.hidden', '.png'), 'hidden.png');
  // The ending is the file's own kind, whatever the name said.
  assert.strictEqual(backgrounds.safeName('photo.png', '.jpg'), 'photo.jpg');
  // Every name made is one the list shows and the face's settings take.
  ['Storm over the lake.jpg', 'sea (2).png', 't- - la mer.webp'].forEach((name) => {
    assert.ok(backgrounds.isName(name), name);
    assert.deepStrictEqual(facelook.plan({}, { 'idle.picture': name }), { changes: { 'face.idle.picture': name } });
  });
});

test('only pictures by name are listed from the folder', () => {
  ['a.jpg', 'A b_c (1).JPEG', 'x.webp', 'y.png'].forEach((n) => assert.ok(backgrounds.isName(n), n));
  ['.hidden.jpg', 'notes.txt', 'a..jpg', 'sub/a.jpg', 'a.gif', '', 'x'.repeat(70) + '.jpg', 'quote".jpg'].forEach((n) => assert.ok(!backgrounds.isName(n), n));
});
