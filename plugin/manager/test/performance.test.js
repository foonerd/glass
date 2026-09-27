'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { PROFILES, boardClass, autoProfile, nameOf } = require('../performance');

test('boards are classed from their model, and given a profile', () => {
  assert.strictEqual(boardClass('Raspberry Pi 5 Model B Rev 1.1', 4, 'aarch64'), 'pi5');
  assert.strictEqual(boardClass('Raspberry Pi Compute Module 5 Rev 1.0', 4, 'aarch64'), 'pi5');
  assert.strictEqual(boardClass('Raspberry Pi 4 Model B Rev 1.4', 4, 'armv7l'), 'pi4');
  assert.strictEqual(boardClass('Raspberry Pi Zero 2 W Rev 1.0', 4, 'aarch64'), 'zero2');
  assert.strictEqual(boardClass('Raspberry Pi 3 Model B Plus Rev 1.3', 4, 'armv7l'), 'pi3');
  assert.strictEqual(boardClass('', 8, 'x86_64'), 'x64');
  assert.strictEqual(boardClass('Some Box', 2, 'aarch64'), 'other');
  assert.strictEqual(autoProfile('pi5', 720), 'standard');
  assert.strictEqual(autoProfile('pi3', 720), 'light');
  assert.strictEqual(autoProfile('pi3', 1080), 'minimal');
  assert.strictEqual(autoProfile('zero2', 480), 'minimal');
  assert.strictEqual(autoProfile('x64', 1080), 'standard');
});

test('values name their profile, or custom', () => {
  assert.strictEqual(nameOf(PROFILES.standard), 'standard');
  assert.strictEqual(nameOf({ frameRate: '30', rotationQuality: 'medium', rotationFps: '8', transitions: true }), 'standard');
  assert.strictEqual(nameOf({ frameRate: 25, rotationQuality: 'medium', rotationFps: 8, transitions: true }), 'custom');
  assert.strictEqual(nameOf({ frameRate: 15, rotationQuality: 'low', rotationFps: 4, transitions: false }), 'minimal');
});
