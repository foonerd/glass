'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { deaths, wait, holdUntil, LAUNCH_MS, LONGEST_MS } = require('../relaunch');

test('deaths at launch are counted in a row; a display that ran or left cleanly starts the count again', function () {
  let count = 0;
  count = deaths(count, { clean: false, ranMs: 300 });
  count = deaths(count, { clean: false, ranMs: 0 });
  assert.equal(count, 2);
  assert.equal(deaths(count, { clean: false, ranMs: LAUNCH_MS }), 0, 'it ran: a death later is not one at launch');
  assert.equal(deaths(count, { clean: true, ranMs: 100 }), 0, 'a clean exit is no death');
  assert.equal(deaths(count, { clean: false, ranMs: LAUNCH_MS - 1 }), 3);
});

test('each death at launch in a row doubles the wait, a minute at most', function () {
  assert.deepEqual([1, 2, 3, 4, 5, 6, 7, 8, 50].map(function (n) { return wait(n, 1000); }), [1000, 2000, 4000, 8000, 16000, 32000, 60000, 60000, 60000]);
  assert.deepEqual([1, 2, 3, 4].map(function (n) { return wait(n, 20000); }), [20000, 40000, 60000, 60000]);
  assert.equal(wait(5, 120000), 120000, 'a screensaver delay longer than a minute is the wait');
  assert.equal(wait(1, 0), 1000, 'never under a second');
  assert.equal(wait(0, 1000), 1000);
  assert.equal(wait(3, 'x'), 4000);
  assert.ok(wait(1e6, 1000) <= LONGEST_MS);
});

test('the hold ends half a cadence before the wait, so the timer\'s next turn starts the display', function () {
  assert.equal(holdUntil(100000, 1, 1000), 100500);
  assert.equal(holdUntil(100000, 3, 1000), 103500);
  assert.equal(holdUntil(100000, 2, 30000), 100000 + 60000 - 15000);
  // Timer turns every second from 100000.9 on: with three deaths the start is the turn 4 s after the death.
  const until = holdUntil(100100, 3, 1000);
  const turns = [100900, 101900, 102900, 103900, 104900];
  assert.equal(turns.find(function (t) { return t >= until; }), 103900);
});
