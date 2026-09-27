'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { trackFolder, trackFilePath } = require('../trackfile');

test('a track location becomes its folder under /mnt', () => {
  assert.strictEqual(trackFolder('music-library/INTERNAL/Queen/Opera/01.flac'), '/mnt/INTERNAL/Queen/Opera');
  assert.strictEqual(trackFolder('mnt/NAS/music/a/b.mp3'), '/mnt/NAS/music/a');
  assert.strictEqual(trackFolder('mnt/USB/x/y.wav'), '/mnt/USB/x');
  assert.strictEqual(trackFolder('http://stream.example/radio'), null);
  assert.strictEqual(trackFolder(''), null);
});

test('a picture name inside the folder is served, anything else is not', () => {
  const uri = 'music-library/INTERNAL/Queen/Opera/01.flac';
  assert.strictEqual(trackFilePath(uri, 'back.png'), '/mnt/INTERNAL/Queen/Opera/back.png');
  assert.strictEqual(trackFilePath(uri, 'Logo.JPG'), '/mnt/INTERNAL/Queen/Opera/Logo.JPG');
  assert.strictEqual(trackFilePath(uri, '../secret.png'), null);
  assert.strictEqual(trackFilePath(uri, 'sub/back.png'), null);
  assert.strictEqual(trackFilePath(uri, 'notes.txt'), null);
  assert.strictEqual(trackFilePath(uri, ''), null);
  assert.strictEqual(trackFilePath('mnt/../etc/x.flac', 'back.png'), null);
});
