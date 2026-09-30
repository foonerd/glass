'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { trackFolder, trackFilePath, trackFileFor } = require('../trackfile');

test('a picture is served only from the folder of the track the player reports', () => {
  const playing = 'music-library/INTERNAL/Queen/Opera/01.flac';
  assert.strictEqual(trackFileFor('mnt/INTERNAL/Queen/Opera/02.flac', 'back.png', playing), '/mnt/INTERNAL/Queen/Opera/back.png');
  assert.strictEqual(trackFileFor('music-library/INTERNAL/Queen/Jazz/01.flac', 'back.png', playing), null);
  assert.strictEqual(trackFileFor(playing, 'back.png', ''), null);
  assert.strictEqual(trackFileFor(playing, 'back.png', 'http://stream.example/radio'), null);
});

test('a track location becomes its folder under /mnt', () => {
  assert.strictEqual(trackFolder('music-library/INTERNAL/Queen/Opera/01.flac'), '/mnt/INTERNAL/Queen/Opera');
  assert.strictEqual(trackFolder('mnt/NAS/music/a/b.mp3'), '/mnt/NAS/music/a');
  assert.strictEqual(trackFolder('mnt/USB/x/y.wav'), '/mnt/USB/x');
  assert.strictEqual(trackFolder('http://stream.example/radio'), null);
  assert.strictEqual(trackFolder('cue://NAS/Music/Songs Without Words/CD1.cue@5'), '/mnt/NAS/Music/Songs Without Words', 'a track inside a cue sheet is in the sheet\'s folder');
  assert.strictEqual(trackFolder('cue://INTERNAL/Album/disc.cue'), '/mnt/INTERNAL/Album');
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
