'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { deaths, wait, holdUntil, base, LAUNCH_MS, LONGEST_MS } = require('../relaunch');

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

test('on a screen of its own the waits begin at a second, whatever the screensaver\'s delay', function () {
  assert.equal(base(true, 30000), 1000);
  assert.equal(base(true, 0), 1000);
  assert.equal(base(false, 30000), 30000, 'over a kiosk the screensaver\'s delay is the base');
  assert.equal(base(false, 0), 1000);
  assert.equal(base(false, 'x'), 1000);
});

test('a face that cannot start has failed three times within seconds, not minutes', function () {
  // The screen's watcher looks every five seconds and starts a display that is gone and not held.
  const third = function (baseMs) {
    let now = 0;
    let held = 0;
    let count = 0;
    for (let tick = 0; tick < 1000; tick++) {
      now = tick * 5000;
      if (now < held) continue;
      count = deaths(count, { clean: false, ranMs: 300 });
      if (count === 3) return now;
      held = holdUntil(now + 300, count, baseMs);
    }
    return Infinity;
  };
  assert.equal(third(base(true, 30000)), 10000, 'three starts on three turns of the watcher');
  assert.ok(third(base(false, 30000)) >= 60000, 'at the screensaver\'s pace it took over a minute');
});
