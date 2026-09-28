'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { minutesOf, periodAt, nextSwitch, parseIso6709, zoneLocationIn, sunTimes, periodBySun, nextSwitchBySun } = require('../cardash');

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

test('a zone table gives a place for the time zone', () => {
  assert.deepStrictEqual(parseIso6709('+513030-0000731'), { lat: 51.5083, lon: -0.1253 });
  assert.deepStrictEqual(parseIso6709('-3352+15113'), { lat: -33.8667, lon: 151.2167 });
  assert.strictEqual(parseIso6709('London'), null);
  const table = '# comment\nGB,GG,IM,JE\t+513030-0000731\tEurope/London\nAU\t-3352+15113\tAustralia/Sydney\tNew South Wales (most areas)\n';
  assert.deepStrictEqual(zoneLocationIn(table, 'Europe/London'), { zone: 'Europe/London', lat: 51.5083, lon: -0.1253 });
  assert.strictEqual(zoneLocationIn(table, 'Etc/UTC'), null);
});

const hm = (d) => d.getUTCHours() * 60 + d.getUTCMinutes();

test('sunrise and sunset come within two minutes of the almanac', () => {
  // London, 21 June 2026: 03:43 and 20:21 UT (04:43 and 21:21 BST).
  const solstice = sunTimes(new Date(2026, 5, 21, 12), 51.5074, -0.1278);
  assert.ok(Math.abs(hm(solstice.rise) - (3 * 60 + 43)) <= 2, 'rise ' + solstice.rise.toISOString());
  assert.ok(Math.abs(hm(solstice.set) - (20 * 60 + 21)) <= 2, 'set ' + solstice.set.toISOString());
  // London, 21 December 2026: 08:04 and 15:53 UT.
  const winter = sunTimes(new Date(2026, 11, 21, 12), 51.5074, -0.1278);
  assert.ok(Math.abs(hm(winter.rise) - (8 * 60 + 4)) <= 2, 'rise ' + winter.rise.toISOString());
  assert.ok(Math.abs(hm(winter.set) - (15 * 60 + 53)) <= 2, 'set ' + winter.set.toISOString());
  // Tromso in June: the midnight sun, no rise and no set.
  assert.strictEqual(sunTimes(new Date(2026, 5, 21, 12), 69.65, 18.96), null);
  // Civil twilight is earlier and later than the horizon.
  const dusk = sunTimes(new Date(2026, 5, 21, 12), 51.5074, -0.1278, -6);
  assert.ok(dusk.rise < solstice.rise && dusk.set > solstice.set);
});

test('by the sun, the period and the next switch follow the day, with an offset', () => {
  const london = [51.5074, -0.1278];
  // Noon UT on the solstice: day; the next switch is the night at sunset plus 30 minutes.
  const noon = new Date(Date.UTC(2026, 5, 21, 12, 0));
  assert.strictEqual(periodBySun(noon, london[0], london[1], 30), 'day');
  const next = nextSwitchBySun(noon, london[0], london[1], 30);
  assert.strictEqual(next.period, 'night');
  assert.ok(Math.abs(hm(next.at) - (20 * 60 + 51)) <= 2, next.at.toISOString());
  // Just before sunrise less 30 minutes: night, and the day comes next.
  const early = new Date(Date.UTC(2026, 5, 21, 3, 0));
  assert.strictEqual(periodBySun(early, london[0], london[1], 30), 'night');
  assert.strictEqual(nextSwitchBySun(early, london[0], london[1], 30).period, 'day');
  // Where the sun neither rises nor sets, nothing is said.
  assert.strictEqual(periodBySun(noon, 69.65, 18.96, 0), null);
});
