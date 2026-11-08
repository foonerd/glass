'use strict';
// The second forecast source: MET Norway's Locationforecast (compact), asked
// when Open-Meteo does not answer. Free, no key, its data under CC BY 4.0,
// its terms asking an identifying user agent. Pure: the address asked, the
// sky codes made WMO codes (the face reads those), and what an answer
// becomes, in the very shape Open-Meteo's reading has (weather.js), so the
// face, the Status tab and the likeness tell no difference but the source.

const FORECAST_URL = 'https://api.met.no/weatherapi/locationforecast/2.0/compact';
const SOURCE = { name: 'MET Norway', url: 'https://www.met.no/en', licence: 'CC BY 4.0' };

function forecastUrl(where) {
  return FORECAST_URL + '?lat=' + where.latitude + '&lon=' + where.longitude;
}

// MET's symbol codes as WMO codes: 0 and 1 clear, 2 partly cloudy, 3
// overcast, 45 fog, 61 to 65 rain, 80 to 82 showers, sleet as rain (66, 67
// heavy), 71 to 75 snow, 85 and 86 snow showers, 95 thunder (96 heavy).
// The suffix (_day, _night, _polartwilight) is the sky's time, not its kind.
const WMO = {
  clearsky: 0, fair: 1, partlycloudy: 2, cloudy: 3, fog: 45,
  lightrain: 61, rain: 63, heavyrain: 65,
  lightrainshowers: 80, rainshowers: 81, heavyrainshowers: 82,
  lightsleet: 66, sleet: 66, heavysleet: 67,
  lightsleetshowers: 66, sleetshowers: 66, heavysleetshowers: 67,
  lightsnow: 71, snow: 73, heavysnow: 75,
  lightsnowshowers: 85, snowshowers: 85, heavysnowshowers: 86
};

function bare(symbol) {
  return String(symbol || '').toLowerCase().replace(/_(day|night|polartwilight)$/, '');
}

function wmo(symbol) {
  const kind = bare(symbol);
  if (!kind) return null;
  if (kind.indexOf('thunder') !== -1) return kind.indexOf('heavy') === 0 ? 96 : 95;
  return Object.prototype.hasOwnProperty.call(WMO, kind) ? WMO[kind] : 3;
}

// Day or night from the symbol's suffix; a symbol without one (cloudy,
// rain, ...) takes the hour: seven to eighteen is day.
function isDay(symbol, hour) {
  const s = String(symbol || '').toLowerCase();
  if (/_night$/.test(s)) return false;
  if (/_(day|polartwilight)$/.test(s)) return true;
  return hour >= 7 && hour <= 18;
}

function number(value, low, high) {
  const n = typeof value === 'number' ? value : parseFloat(value);
  return Number.isFinite(n) && n >= low && n <= high ? n : null;
}

function degrees(celsius, unit) {
  if (celsius === null) return null;
  return unit === 'F' ? Math.round((celsius * 9 / 5 + 32) * 10) / 10 : celsius;
}

// The sky an entry tells: the next hour's where the entry has one, else the
// next six hours', else the next twelve.
function symbolOf(data) {
  const pick = function (span) { return data && data[span] && data[span].summary && data[span].summary.symbol_code; };
  return pick('next_1_hours') || pick('next_6_hours') || pick('next_12_hours') || null;
}

// What an answer becomes: now (the temperature and the sky of the first
// entry, day or night), today (the day's sky, its low and its high; no
// chance of rain, which the compact answer does not carry), the next 24
// hourly entries from the next whole hour (each its hour, temperature,
// code and day or night), and the week (each day its weekday, code, low
// and high, from the entries of that day, at least two). MET's times are
// UTC without the place's offset, so the hours and the days are the
// player's own local time (`local(ms)` gives {hour, weekday, date} for an
// instant; the player's clock unless given). Null where the answer is no
// forecast.
function reading(body, where, unit, nowMs, local) {
  let json;
  try { json = JSON.parse(String(body)); } catch (e) { return null; }
  const series = json && json.properties && Array.isArray(json.properties.timeseries) ? json.properties.timeseries : null;
  if (!series || !series.length) return null;
  const at = local || function (ms) { const d = new Date(ms); return { hour: d.getHours(), weekday: d.getDay(), date: d.getFullYear() + '-' + (d.getMonth() + 1) + '-' + d.getDate() }; };
  const entries = [];
  for (let i = 0; i < series.length; i++) {
    const e = series[i];
    const when = Date.parse(String(e && e.time || ''));
    const temp = number(e && e.data && e.data.instant && e.data.instant.details && e.data.instant.details.air_temperature, -150, 150);
    if (!Number.isFinite(when) || temp === null) continue;
    entries.push({ when: when, temp: temp, symbol: symbolOf(e.data), hourly: !!(e.data && e.data.next_1_hours), local: at(when) });
  }
  if (!entries.length) return null;
  const hours = [];
  for (let i = 0; i < entries.length && hours.length < 24; i++) {
    const e = entries[i];
    if (!e.hourly || e.when < nowMs || e.symbol === null) continue;
    hours.push({ hour: e.local.hour, temp: degrees(e.temp, unit), code: wmo(e.symbol), day: isDay(e.symbol, e.local.hour) });
  }
  const byDay = [];
  entries.forEach(function (e) {
    let day = byDay.length ? byDay[byDay.length - 1] : null;
    if (!day || day.date !== e.local.date) { day = { date: e.local.date, weekday: e.local.weekday, entries: [] }; byDay.push(day); }
    day.entries.push(e);
  });
  const days = [];
  byDay.forEach(function (day) {
    if (days.length >= 7 || day.entries.length < 2) return;
    let low = Infinity, high = -Infinity, nearest = null;
    day.entries.forEach(function (e) {
      if (e.temp < low) low = e.temp;
      if (e.temp > high) high = e.temp;
      if (e.symbol !== null && (!nearest || Math.abs(e.local.hour - 12) < Math.abs(nearest.local.hour - 12))) nearest = e;
    });
    if (!nearest) return;
    days.push({ weekday: day.weekday, code: wmo(nearest.symbol), low: degrees(low, unit), high: degrees(high, unit) });
  });
  if (!days.length) return null;
  const first = entries[0];
  const code = first.symbol !== null ? wmo(first.symbol) : days[0].code;
  return {
    place: where.name,
    unit: unit,
    now: degrees(first.temp, unit),
    code: code,
    day: isDay(first.symbol, first.local.hour),
    today: days[0].code,
    low: days[0].low,
    high: days[0].high,
    rain: null,
    at: Math.floor(nowMs / 1000),
    hours: hours,
    days: days,
    source: SOURCE.name
  };
}

module.exports = { SOURCE: SOURCE, forecastUrl: forecastUrl, wmo: wmo, isDay: isDay, reading: reading };
