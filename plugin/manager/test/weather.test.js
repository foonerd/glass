'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const weather = require('../weather');

const KRAKOW = { name: 'Kraków, Lesser Poland, Poland', latitude: 50.0614, longitude: 19.9366 };
// Read at 1760000000999, 08:53:21 UTC on a Thursday, two hours ahead at the place.
const ANSWER = JSON.stringify({
  utc_offset_seconds: 7200,
  current: { temperature_2m: 13.6, weather_code: 3, is_day: 1 },
  daily: { time: ['2025-10-09', '2025-10-10'], weather_code: [61, 2], temperature_2m_max: [17.2, 15.0], temperature_2m_min: [8.9, 7.1], precipitation_probability_max: [64, 10] },
  hourly: { time: ['2025-10-09T10:00', '2025-10-09T11:00', '2025-10-09T12:00'], temperature_2m: [12.1, 13.0, 14.2], weather_code: [3, 61, 2], is_day: [1, 1, 0] }
});

function scratch() {
  return path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'glass-weather-')), 'weather.json');
}

// A keeper with a clock, a request and a timer of the test's own.
function keeper(file, answers) {
  const world = { now: 1760000000000, asked: [], pushed: [], timers: [] };
  const forecast = new weather.Forecast({
    file: file,
    now: function () { return world.now; },
    get: function (url) {
      world.asked.push(url);
      const next = answers.shift();
      return next instanceof Error ? Promise.reject(next) : Promise.resolve({ body: Buffer.from(next) });
    },
    push: function (message) { world.pushed.push(message); },
    later: function (fn, ms) { const t = { fn: fn, ms: ms }; world.timers.push(t); return t; },
    never: function (t) { world.timers = world.timers.filter(function (x) { return x !== t; }); }
  });
  // A timer comes due: it leaves the world's list as a real one would, then runs.
  world.fire = function () { const t = world.timers.shift(); t.fn(); };
  return { forecast: forecast, world: world };
}

test('a place is a name and where it is, and nothing else', function () {
  assert.deepStrictEqual(weather.place({ name: '  Kraków\n', latitude: '50.06143', longitude: 19.93658 }), { name: 'Kraków', latitude: 50.0614, longitude: 19.9366 });
  assert.strictEqual(weather.place({ name: 'Nowhere', latitude: 91, longitude: 0 }), null);
  assert.strictEqual(weather.place({ name: '', latitude: 1, longitude: 1 }), null);
  assert.strictEqual(weather.place({ name: 'x', latitude: 'north', longitude: 1 }), null);
  assert.strictEqual(weather.place(null), null);
  assert.strictEqual(weather.unit('f'), 'F');
  assert.strictEqual(weather.unit('kelvin'), 'C');
});

test('the addresses asked name the place, the day and the unit', function () {
  const url = weather.forecastUrl(KRAKOW, 'F');
  assert.ok(url.startsWith('https://api.open-meteo.com/v1/forecast?latitude=50.0614&longitude=19.9366&'));
  assert.ok(url.includes('forecast_days=7') && url.includes('hourly=temperature_2m,weather_code,is_day') && url.includes('temperature_unit=fahrenheit') && url.includes('timezone=auto'));
  assert.ok(weather.forecastUrl(KRAKOW, 'C').includes('temperature_unit=celsius'));
  assert.strictEqual(weather.searchUrl(' Kraków & co ', 'pl'), 'https://geocoding-api.open-meteo.com/v1/search?name=Krak%C3%B3w%20%26%20co&count=8&format=json&language=pl');
  assert.ok(weather.searchUrl('x', 'not a language').endsWith('language=en'));
});

test('a search answer becomes places that tell apart', function () {
  const body = JSON.stringify({ results: [
    { name: 'Springfield', admin1: 'Illinois', country: 'United States', latitude: 39.8, longitude: -89.64 },
    { name: 'Springfield', admin1: 'Missouri', country: 'United States', latitude: 37.2, longitude: -93.3 },
    { name: 'Luxembourg', admin1: 'Luxembourg', country: 'Luxembourg', latitude: 49.61, longitude: 6.13 },
    { name: 'Broken', latitude: 'x', longitude: 0 }
  ] });
  assert.deepStrictEqual(weather.found(body).map(function (p) { return p.name; }), ['Springfield, Illinois, United States', 'Springfield, Missouri, United States', 'Luxembourg']);
  assert.deepStrictEqual(weather.found('{}'), []);
  assert.deepStrictEqual(weather.found('not json'), []);
});

test('a forecast answer becomes now and today', function () {
  assert.deepStrictEqual(weather.reading(ANSWER, KRAKOW, 'C', 1760000000999), {
    place: KRAKOW.name, unit: 'C', now: 13.6, code: 3, day: true, today: 61, low: 8.9, high: 17.2, rain: 64, at: 1760000000, source: 'Open-Meteo',
    // The hours from the next whole hour at the place (10:53 there: 11 and 12), the days from today, a Thursday.
    hours: [{ hour: 11, temp: 13.0, code: 61, day: true }, { hour: 12, temp: 14.2, code: 2, day: false }],
    days: [{ weekday: 4, code: 61, low: 8.9, high: 17.2 }, { weekday: 5, code: 2, low: 7.1, high: 15.0 }]
  });
  const bare = JSON.stringify({ current: { is_day: 0 }, daily: { weather_code: [0], temperature_2m_max: [1], temperature_2m_min: [-4] } });
  const read = weather.reading(bare, KRAKOW, 'C', 0);
  assert.deepStrictEqual([read.now, read.code, read.day, read.rain], [null, 0, false, null], 'no current values: the day\'s code stands in, night as said');
  assert.deepStrictEqual([read.hours, read.days], [[], []], 'no hourly answer and no dated days: none to show');
  assert.strictEqual(weather.reading('{"daily":{}}', KRAKOW, 'C', 0), null);
  assert.strictEqual(weather.reading('<html>', KRAKOW, 'C', 0), null);
  assert.deepStrictEqual(weather.message(null), { kind: 'weather', off: true });
  assert.strictEqual(weather.message(read).kind, 'weather');
});

test('off until a place is chosen: nothing is asked, and the displays hear there is none', async function () {
  const { forecast, world } = keeper(scratch(), []);
  await forecast.start();
  assert.deepStrictEqual(world.asked, []);
  assert.deepStrictEqual(world.pushed, [{ kind: 'weather', off: true }]);
  assert.deepStrictEqual(world.timers, []);
  assert.strictEqual(forecast.state().place, null);
});

test('a place chosen is kept, asked for at once, pushed, and asked for again in half an hour', async function () {
  const file = scratch();
  const { forecast, world } = keeper(file, [ANSWER, ANSWER]);
  await forecast.start();
  const state = await forecast.set({ place: KRAKOW, unit: 'C' });
  assert.strictEqual(world.asked.length, 1);
  assert.strictEqual(state.reading.high, 17.2);
  assert.strictEqual(world.pushed[world.pushed.length - 1].today, 61);
  assert.deepStrictEqual(world.timers.map(function (t) { return t.ms; }), [weather.EVERY_MS]);
  assert.deepStrictEqual(JSON.parse(fs.readFileSync(file, 'utf8')).place, KRAKOW);
  // The timer fires: asked again.
  world.fire();
  await forecast.asking;
  assert.strictEqual(world.asked.length, 2);
  // A new start reads what was kept and shows it before the answer is in.
  const again = keeper(file, [ANSWER]);
  const started = again.forecast.start();
  assert.strictEqual(again.world.pushed[0].high, 17.2);
  await started;
  assert.strictEqual(again.world.asked.length, 1);
  assert.strictEqual(forecast.set({ place: { name: 'x' } }), null, 'a place that is none is refused');
});

test('a failure keeps the last reading while it is fresh, asks again sooner, and lets go of a stale one', async function () {
  // Both sources down at each refresh: the first's failure, then the second's.
  const { forecast, world } = keeper(scratch(), [ANSWER, new Error('down'), new Error('down'), new Error('down'), new Error('down')]);
  await forecast.start();
  await forecast.set({ place: KRAKOW });
  world.fire();
  await forecast.asking;
  assert.strictEqual(forecast.state().error, 'Open-Meteo: down; MET Norway: down');
  assert.strictEqual(world.pushed[world.pushed.length - 1].high, 17.2, 'still shown');
  assert.deepStrictEqual(world.timers.map(function (t) { return t.ms; }), [weather.RETRY_MS]);
  world.now += weather.STALE_MS + 1000;
  world.fire();
  await forecast.asking;
  assert.deepStrictEqual(world.pushed[world.pushed.length - 1], { kind: 'weather', off: true });
  assert.strictEqual(forecast.state().reading, null);
});

test('turned off: the place is forgotten, the timer stopped, the displays told', async function () {
  const file = scratch();
  const { forecast, world } = keeper(file, [ANSWER]);
  await forecast.start();
  await forecast.set({ place: KRAKOW, unit: 'F' });
  const state = await forecast.set({ place: null });
  assert.strictEqual(state.place, null);
  assert.deepStrictEqual(world.timers, []);
  assert.deepStrictEqual(world.pushed[world.pushed.length - 1], { kind: 'weather', off: true });
  assert.strictEqual(JSON.parse(fs.readFileSync(file, 'utf8')).place, null);
  assert.strictEqual(state.unit, 'F', 'the unit stays as chosen');
});

test('a unit changed asks again and drops the reading in the other unit', async function () {
  const { forecast, world } = keeper(scratch(), [ANSWER, ANSWER]);
  await forecast.start();
  await forecast.set({ place: KRAKOW });
  const state = await forecast.set({ unit: 'F' });
  assert.strictEqual(world.asked.length, 2);
  assert.ok(world.asked[1].includes('fahrenheit'));
  assert.strictEqual(state.reading.unit, 'F');
});

test('a search asks the source and hands the places back; two letters at least', async function () {
  const body = JSON.stringify({ results: [{ name: 'Kraków', admin1: 'Lesser Poland', country: 'Poland', latitude: 50.06143, longitude: 19.93658 }] });
  const { forecast, world } = keeper(scratch(), [body]);
  assert.deepStrictEqual(await forecast.search('K', 'en'), []);
  assert.deepStrictEqual(await forecast.search('Krak', 'pl'), [KRAKOW]);
  assert.strictEqual(world.asked.length, 1);
});

// MET Norway's compact answer, the second source: hourly entries with the
// next hour's sky, then six-hourly ones; times in UTC, temperatures in
// Celsius. Read at 1760000000999 (Thursday 9 October 2025, 08:53:21 UTC).
const MET = JSON.stringify({ type: 'Feature', properties: { meta: { units: { air_temperature: 'celsius' } }, timeseries: [
  { time: '2025-10-09T08:00:00Z', data: { instant: { details: { air_temperature: 11.0 } }, next_1_hours: { summary: { symbol_code: 'cloudy' } }, next_6_hours: { summary: { symbol_code: 'lightrain' } } } },
  { time: '2025-10-09T09:00:00Z', data: { instant: { details: { air_temperature: 12.5 } }, next_1_hours: { summary: { symbol_code: 'lightrainshowers_day' } } } },
  { time: '2025-10-09T10:00:00Z', data: { instant: { details: { air_temperature: 14.0 } }, next_1_hours: { summary: { symbol_code: 'partlycloudy_day' } } } },
  { time: '2025-10-09T20:00:00Z', data: { instant: { details: { air_temperature: 9.0 } }, next_1_hours: { summary: { symbol_code: 'clearsky_night' } } } },
  { time: '2025-10-10T06:00:00Z', data: { instant: { details: { air_temperature: 4.0 } }, next_6_hours: { summary: { symbol_code: 'heavysnowshowers_day' } } } },
  { time: '2025-10-10T12:00:00Z', data: { instant: { details: { air_temperature: 7.5 } }, next_6_hours: { summary: { symbol_code: 'sleet' } } } },
  { time: '2025-10-11T12:00:00Z', data: { instant: { details: { air_temperature: 6.0 } }, next_6_hours: { summary: { symbol_code: 'rainandthunder' } } } }
] } });
// The player's clock two hours ahead of UTC, as the place's.
const LOCAL = function (ms) { const d = new Date(ms + 7200000); return { hour: d.getUTCHours(), weekday: d.getUTCDay(), date: d.toISOString().slice(0, 10) }; };

test('when Open-Meteo does not answer, MET Norway is asked in the same refresh and the reading says so; both failing is the failure as before', async function () {
  const file = scratch();
  const { forecast, world } = keeper(file, [new Error('ENETUNREACH'), MET]);
  forecast.local = LOCAL;
  await forecast.start();
  await forecast.set({ place: KRAKOW });
  assert.strictEqual(world.asked.length, 2);
  assert.ok(/api\.open-meteo\.com/.test(world.asked[0]) && /api\.met\.no\/weatherapi\/locationforecast\/2\.0\/compact\?lat=50\.0614&lon=19\.9366$/.test(world.asked[1]), 'the first, then the second');
  const read = forecast.current();
  assert.strictEqual(read.source, 'MET Norway');
  assert.deepStrictEqual([read.now, read.code, read.day, read.today, read.low, read.high, read.rain], [11.0, 3, true, 2, 9.0, 14.0, null], 'now from the first entry; today its low and high and the sky of the entry nearest midday; no chance of rain');
  // The hours from 09:00 UTC on (now is 08:53), labelled in the player's time (two ahead); the days from today, a Thursday.
  assert.deepStrictEqual(read.hours, [{ hour: 11, temp: 12.5, code: 80, day: true }, { hour: 12, temp: 14.0, code: 2, day: true }, { hour: 22, temp: 9.0, code: 0, day: false }]);
  assert.deepStrictEqual(read.days, [{ weekday: 4, code: 2, low: 9.0, high: 14.0 }, { weekday: 5, code: 66, low: 4.0, high: 7.5 }], 'the sky nearest midday (14:00 over 08:00 on the Friday); a day of one entry is left out');
  const state = forecast.state();
  assert.deepStrictEqual([state.served.name, state.error, state.firstError, state.sources.map(function (s) { return s.name; })], ['MET Norway', '', 'ENETUNREACH', ['Open-Meteo', 'MET Norway']]);
  assert.strictEqual(world.pushed[world.pushed.length - 1].source, 'MET Norway');
  assert.strictEqual(JSON.parse(fs.readFileSync(file, 'utf8')).reading.source, 'MET Norway', 'kept with its source');
  // Open-Meteo back at the next refresh: the first serves again, nothing said of the second.
  world.now += weather.EVERY_MS;
  world.asked.length = 0;
  forecast.get = function (url) { world.asked.push(url); return Promise.resolve({ body: Buffer.from(ANSWER) }); };
  world.fire();
  await forecast.asking;
  assert.strictEqual(world.asked.length, 1);
  assert.deepStrictEqual([forecast.state().served.name, forecast.state().firstError], ['Open-Meteo', '']);
  // Both silent: the failure names both, the last reading stands while fresh, the retry is sooner.
  world.now += weather.EVERY_MS;
  forecast.get = function (url) { world.asked.push(url); return Promise.reject(new Error(/met\.no/.test(url) ? 'ETIMEDOUT' : 'ENETUNREACH')); };
  world.fire();
  await forecast.asking;
  assert.match(forecast.state().error, /^Open-Meteo: ENETUNREACH; MET Norway: ETIMEDOUT$/);
  assert.strictEqual(forecast.current().source, 'Open-Meteo', 'the last reading stands');
  assert.strictEqual(world.timers[0].ms, weather.RETRY_MS);
  // The player identifies itself to every service.
  const seen = [];
  forecast.get = function (url, options) { seen.push(options.headers['user-agent']); return Promise.resolve({ body: Buffer.from(ANSWER) }); };
  await forecast.refresh();
  assert.deepStrictEqual(seen, ['glass-volumio/0 (+https://github.com/foonerd/glass)']);
  assert.strictEqual(weather.agent('0.11.35'), 'glass-volumio/0.11.35 (+https://github.com/foonerd/glass)');
});

test('a place search falls back to Nominatim when Open-Meteo\'s search does not answer, the places named alike', async function () {
  const nominatim = JSON.stringify([
    { name: 'Lisbon', display_name: 'Lisbon, Portugal', lat: '38.7077507', lon: '-9.1365919' },
    { name: 'Lisbon', display_name: 'Lisbon, Franklin Township, Linn County, Iowa, United States', lat: '41.9211165', lon: '-91.3854913' },
    { name: 'Lisbon', display_name: 'Lisbon, Ransom County, North Dakota, 58054, United States', lat: '46.4416', lon: '-97.6812' },
    { name: '', display_name: 'nowhere', lat: 'x', lon: 'y' }
  ]);
  const { forecast, world } = keeper(scratch(), [new Error('ENETUNREACH'), nominatim]);
  assert.deepStrictEqual(await forecast.search('Lisbon', 'en'), [
    { name: 'Lisbon, Portugal', latitude: 38.7078, longitude: -9.1366 },
    { name: 'Lisbon, Iowa, United States', latitude: 41.9211, longitude: -91.3855 },
    { name: 'Lisbon, North Dakota, United States', latitude: 46.4416, longitude: -97.6812 }
  ], 'the region and the country, never a postcode');
  assert.ok(/nominatim\.openstreetmap\.org\/search\?q=Lisbon&format=jsonv2&limit=8&accept-language=en$/.test(world.asked[1]));
  const neither = keeper(scratch(), [new Error('ENETUNREACH'), new Error('ETIMEDOUT')]);
  await assert.rejects(neither.forecast.search('Lisbon', 'en'), { message: 'ETIMEDOUT' });
});
