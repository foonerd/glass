'use strict';
const test = require('node:test');
const assert = require('node:assert');
const metno = require('../metno');

test('MET Norway\'s sky symbols become the WMO codes the face reads, the suffix dropped, a stranger a cloud', function () {
  const rows = [
    ['clearsky_day', 0], ['clearsky_night', 0], ['clearsky_polartwilight', 0], ['fair_day', 1], ['partlycloudy_night', 2], ['cloudy', 3], ['fog', 45],
    ['lightrain', 61], ['rain', 63], ['heavyrain', 65],
    ['lightrainshowers_day', 80], ['rainshowers_night', 81], ['heavyrainshowers_day', 82],
    ['lightsleet', 66], ['sleet', 66], ['heavysleet', 67], ['lightsleetshowers_night', 66], ['sleetshowers_day', 66], ['heavysleetshowers_day', 67],
    ['lightsnow', 71], ['snow', 73], ['heavysnow', 75], ['lightsnowshowers_day', 85], ['snowshowers_night', 85], ['heavysnowshowers_day', 86],
    ['rainandthunder', 95], ['lightrainshowersandthunder_day', 95], ['sleetandthunder', 95], ['heavysnowshowersandthunder_night', 96], ['heavyrainandthunder', 96],
    ['somethingnew_day', 3], ['SNOW', 73]
  ];
  rows.forEach(function (row) { assert.strictEqual(metno.wmo(row[0]), row[1], row[0]); });
  assert.strictEqual(metno.wmo(''), null);
  assert.strictEqual(metno.wmo(null), null);
});

test('day or night from the symbol\'s suffix, else from the hour', function () {
  assert.strictEqual(metno.isDay('partlycloudy_day', 23), true);
  assert.strictEqual(metno.isDay('partlycloudy_polartwilight', 23), true);
  assert.strictEqual(metno.isDay('clearsky_night', 12), false);
  assert.strictEqual(metno.isDay('cloudy', 12), true);
  assert.strictEqual(metno.isDay('rain', 6), false);
  assert.strictEqual(metno.isDay('rain', 18), true);
  assert.strictEqual(metno.isDay('rain', 19), false);
});

test('the address names the place; an answer that is no forecast is null; Fahrenheit is converted here', function () {
  assert.strictEqual(metno.forecastUrl({ latitude: 50.0614, longitude: 19.9366 }), 'https://api.met.no/weatherapi/locationforecast/2.0/compact?lat=50.0614&lon=19.9366');
  assert.strictEqual(metno.SOURCE.name, 'MET Norway');
  const where = { name: 'Somewhere' };
  assert.strictEqual(metno.reading('<html>', where, 'C', 0), null);
  assert.strictEqual(metno.reading('{"properties":{"timeseries":[]}}', where, 'C', 0), null);
  assert.strictEqual(metno.reading(JSON.stringify({ properties: { timeseries: [{ time: '2025-10-09T08:00:00Z', data: { instant: { details: {} } } }] } }), where, 'C', 0), null, 'no temperature: no entry, no forecast');
  const one = JSON.stringify({ properties: { timeseries: [
    { time: '2025-10-09T08:00:00Z', data: { instant: { details: { air_temperature: 10 } }, next_1_hours: { summary: { symbol_code: 'fog' } } } },
    { time: '2025-10-09T09:00:00Z', data: { instant: { details: { air_temperature: 20 } }, next_1_hours: { summary: { symbol_code: 'fair_day' } } } }
  ] } });
  const local = function (ms) { const d = new Date(ms); return { hour: d.getUTCHours(), weekday: d.getUTCDay(), date: d.toISOString().slice(0, 10) }; };
  const f = metno.reading(one, where, 'F', Date.parse('2025-10-09T08:30:00Z'), local);
  assert.deepStrictEqual([f.unit, f.now, f.low, f.high, f.hours[0].temp, f.source], ['F', 50, 50, 68, 68, 'MET Norway']);
  assert.deepStrictEqual([f.code, f.day, f.today, f.days.length, f.hours.length], [45, true, 1, 1, 1], 'now is the first entry (fog at 08, day by the hour); today the sky nearest midday (fair at 09 over fog at 08)');
  const c = metno.reading(one, where, 'C', Date.parse('2025-10-09T08:30:00Z'), local);
  assert.deepStrictEqual([c.now, c.low, c.high, c.at], [10, 10, 20, Math.floor(Date.parse('2025-10-09T08:30:00Z') / 1000)]);
});
