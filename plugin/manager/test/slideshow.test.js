'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const slideshow = require('../slideshow');

const pictures = [{ name: 'a.jpg', stamp: 1 }, { name: 'b.jpg', stamp: 2 }, { name: 'c.jpg', stamp: 3 }];

test('the slideshow is on when the switch is, a picture is chosen and the folder holds more than one; the lines read with their bounds and defaults', () => {
  const on = slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'b.jpg' }, pictures, true);
  assert.equal(on.on, true);
  assert.deepEqual(on.names, ['a.jpg', 'b.jpg', 'c.jpg']);
  assert.equal(on.chosen, 'b.jpg');
  assert.equal(on.every, slideshow.EVERY_MIN, 'ten minutes unless said');
  assert.equal(on.random, false);
  assert.equal(on.transition, 'fade');
  assert.equal(on.ms, slideshow.MS_DEFAULT);
  assert.equal(on.motion, false);
  assert.equal(slideshow.settings({ 'face.idle.slide': 'off', 'face.idle.picture': 'b.jpg' }, pictures, true).on, false, 'the switch off');
  assert.equal(slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': '' }, pictures, true).on, false, 'no picture chosen');
  assert.equal(slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg' }, pictures.slice(0, 1), true).on, false, 'one picture alone is no slideshow');
  const said = slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.every': '900', 'face.idle.slide.random': 'on', 'face.idle.slide.transition': 'merge', 'face.idle.slide.ms': '20000', 'face.idle.slide.motion': 'on' }, pictures, true);
  assert.equal(said.every, 240, 'bounded to four hours');
  assert.equal(said.random, true);
  assert.equal(said.transition, 'merge');
  assert.equal(said.ms, 10000, 'bounded to ten seconds');
  assert.equal(said.motion, true);
  assert.equal(slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.motion': 'on' }, pictures, false).motion, false, 'the profile holds the motion off');
  assert.equal(slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.transition': 'wipe', 'face.idle.slide.every': '0' }, pictures, true).transition, 'fade', 'a transition not known is the fade');
  assert.equal(slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.every': '0' }, pictures, true).every, 1, 'a minute at least');
});

test('the next picture: the folder\'s order wrapping, or a random one never the same twice in a row', () => {
  const names = ['a.jpg', 'b.jpg', 'c.jpg'];
  assert.equal(slideshow.next(names, 'a.jpg', false), 'b.jpg');
  assert.equal(slideshow.next(names, 'c.jpg', false), 'a.jpg', 'wraps');
  assert.equal(slideshow.next(names, 'gone.jpg', false), 'a.jpg', 'a name not in the folder starts at the first');
  assert.equal(slideshow.next(names, 'a.jpg', true, () => 0), 'b.jpg');
  assert.equal(slideshow.next(names, 'a.jpg', true, () => 0.99), 'c.jpg');
  for (let i = 0; i < 50; i++) assert.notEqual(slideshow.next(names, 'b.jpg', true), 'b.jpg', 'never the same twice');
  assert.equal(slideshow.next(['only.jpg'], 'only.jpg', true), 'only.jpg');
  assert.equal(slideshow.next([], 'x', false), '');
});

test('the line names the picture, its stamped address, the change, the motion and the span; an empty name hands back to the look', () => {
  const s = slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.every': '2', 'face.idle.slide.transition': 'merge', 'face.idle.slide.ms': '800', 'face.idle.slide.motion': 'on' }, pictures, true);
  assert.deepEqual(slideshow.line('b.jpg', 2, s), { kind: 'picture', name: 'b.jpg', url: '/api/backgrounds/b.jpg/file?v=2', stamp: 2, transition: 'merge', ms: 800, motion: true, span: 120000 });
  const none = slideshow.settings({ 'face.idle.slide': 'on', 'face.idle.picture': 'a.jpg', 'face.idle.slide.transition': 'none' }, pictures, true);
  assert.equal(slideshow.line('a b.jpg', 0, none).ms, 0, 'no change takes no time');
  assert.equal(slideshow.line('a b.jpg', 0, none).url, '/api/backgrounds/a%20b.jpg/file?v=0');
  assert.deepEqual(slideshow.line('', 0, s), { kind: 'picture', name: '' });
});
