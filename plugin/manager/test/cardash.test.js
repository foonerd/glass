'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { minutesOf, periodAt, nextSwitch } = require('../cardash');

const at = (h, m) => new Date(2026, 8, 28, h, m, 30, 0);

test('a time is HH:MM on the clock', () => {
  assert.strictEqual(minutesOf('07:00'), 420);
  assert.strictEqual(minutesOf(' 23:59 '), 1439);
  assert.strictEqual(minutesOf('24:00'), null);
  assert.strictEqual(minutesOf('7:00'), null);
  assert.strictEqual(minutesOf(''), null);
});

test('the period follows the two times, across midnight too', () => {
  assert.strictEqual(periodAt(at(12, 0), '07:00', '20:00'), 'day');
  assert.strictEqual(periodAt(at(6, 59), '07:00', '20:00'), 'night');
  assert.strictEqual(periodAt(at(7, 0), '07:00', '20:00'), 'day');
  assert.strictEqual(periodAt(at(20, 0), '07:00', '20:00'), 'night');
  assert.strictEqual(periodAt(at(23, 30), '07:00', '20:00'), 'night');
  // A day that starts late and a night that starts early: the day wraps midnight.
  assert.strictEqual(periodAt(at(23, 0), '22:00', '04:00'), 'day');
  assert.strictEqual(periodAt(at(2, 0), '22:00', '04:00'), 'day');
  assert.strictEqual(periodAt(at(12, 0), '22:00', '04:00'), 'night');
  assert.strictEqual(periodAt(at(12, 0), '07:00', '07:00'), null, 'two equal times mean nothing');
  assert.strictEqual(periodAt(at(12, 0), 'noon', '20:00'), null);
});

test('the next switch is the nearest of the two times ahead', () => {
  let next = nextSwitch(at(12, 0), '07:00', '20:00');
  assert.strictEqual(next.period, 'night');
  assert.strictEqual(next.at.getHours(), 20);
  assert.strictEqual(next.at.getDate(), 28);
  next = nextSwitch(at(21, 0), '07:00', '20:00');
  assert.strictEqual(next.period, 'day');
  assert.strictEqual(next.at.getDate(), 29, 'tomorrow morning');
  next = nextSwitch(at(7, 0), '07:00', '20:00');
  assert.strictEqual(next.period, 'night', 'the minute that has begun does not count again');
  assert.strictEqual(nextSwitch(at(7, 0), '07:00', '07:00'), null);
});
