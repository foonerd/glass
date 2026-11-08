'use strict';
// The forecast for one place of the user's choosing, for the face to draw
// beside its clock when nothing plays. Off until a place is chosen: before
// that the player asks nothing of anyone. The source is Open-Meteo
// (https://open-meteo.com, its data under CC BY 4.0): free for private use,
// no account and no key; the place search is its geocoding service. When
// Open-Meteo does not answer (a network that cannot reach it, an error, a
// timeout) MET Norway is asked in the same refresh (metno.js), and the
// place search falls back to OpenStreetMap's Nominatim; the next refresh
// asks Open-Meteo again. The reading says which source served it, and the
// player identifies itself to every service as their terms ask.
//
// Pure at the top: the addresses asked, what an answer becomes, what a
// place may be. The keeper below does the asking and the keeping, with the
// request, the clock and the timer handed in, so it is tested without a
// network.

const fs = require('fs');
const path = require('path');
const metno = require('./metno');

const FORECAST_URL = 'https://api.open-meteo.com/v1/forecast';
const SEARCH_URL = 'https://geocoding-api.open-meteo.com/v1/search';
const SECOND_SEARCH_URL = 'https://nominatim.openstreetmap.org/search';
const SOURCE = { name: 'Open-Meteo', url: 'https://open-meteo.com/', licence: 'CC BY 4.0' };
// The sources in the order asked: the first at every refresh, the second
// when the first does not answer.
const SOURCES = [SOURCE, metno.SOURCE];
// What the player says it is to the services (MET Norway's terms ask for
// the application's name and version).
function agent(version) {
  return 'glass-volumio/' + (version || '0') + ' (+https://github.com/foonerd/glass)';
}
// A forecast is asked for this often, sooner after a failure, and one
// older than the last of these is no longer shown.
const EVERY_MS = 30 * 60 * 1000;
const RETRY_MS = 5 * 60 * 1000;
const STALE_MS = 3 * 60 * 60 * 1000;
const LIMIT_BYTES = 256 * 1024;
const TIMEOUT_MS = 15000;

function number(value, low, high) {
  const n = typeof value === 'number' ? value : parseFloat(value);
  return Number.isFinite(n) && n >= low && n <= high ? n : null;
}

// A place as it is kept: a name to show and where it is. Null for
// anything else.
function place(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const name = String(raw.name || '').replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 80);
  const latitude = number(raw.latitude, -90, 90);
  const longitude = number(raw.longitude, -180, 180);
  if (!name || latitude === null || longitude === null) return null;
  return { name: name, latitude: Math.round(latitude * 1e4) / 1e4, longitude: Math.round(longitude * 1e4) / 1e4 };
}

// Degrees Celsius unless Fahrenheit is asked for.
function unit(raw) {
  return String(raw || '').trim().toUpperCase() === 'F' ? 'F' : 'C';
}

function forecastUrl(where, degrees) {
  return FORECAST_URL + '?latitude=' + where.latitude + '&longitude=' + where.longitude +
    '&current=temperature_2m,weather_code,is_day' +
    '&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max' +
    '&hourly=temperature_2m,weather_code,is_day' +
    '&forecast_days=7&timezone=auto&temperature_unit=' + (degrees === 'F' ? 'fahrenheit' : 'celsius');
}

function searchUrl(name, language) {
  return SEARCH_URL + '?name=' + encodeURIComponent(String(name || '').trim().slice(0, 80)) +
    '&count=8&format=json&language=' + (/^[a-z]{2}$/.test(String(language || '')) ? language : 'en');
}

// The second search, Nominatim's, asked when the first does not answer.
function secondSearchUrl(name, language) {
  return SECOND_SEARCH_URL + '?q=' + encodeURIComponent(String(name || '').trim().slice(0, 80)) +
    '&format=jsonv2&limit=8&accept-language=' + (/^[a-z]{2}$/.test(String(language || '')) ? language : 'en');
}

// The places Nominatim answered with, named as the first search names
// them: the place, then the last two parts of its address (a region and
// the country, or the country alone) that are not the name again and not
// a postcode (a part with a digit in it).
function foundSecond(body) {
  let json;
  try { json = JSON.parse(String(body)); } catch (e) { return []; }
  const results = Array.isArray(json) ? json : [];
  return results.map(function (r) {
    const name = String((r && r.name) || '').trim();
    const chain = String((r && r.display_name) || '').split(',').map(function (s) { return s.trim(); }).filter(function (s) { return s && !/\d/.test(s); });
    const tail = chain.slice(-2).filter(function (s) { return s !== name; });
    const parts = [name].concat(tail).filter(function (s, i, all) { return s && all.indexOf(s) === i; });
    return place({ name: parts.join(', '), latitude: r && r.lat, longitude: r && r.lon });
  }).filter(Boolean).slice(0, 8);
}

// The places a search answered with, each named so that two towns of one
// name tell apart: the town, its region, its country.
function found(body) {
  let json;
  try { json = JSON.parse(String(body)); } catch (e) { return []; }
  const results = json && Array.isArray(json.results) ? json.results : [];
  return results.map(function (r) {
    const parts = [r && r.name, r && r.admin1, r && r.country].filter(function (s, i, all) { return s && all.indexOf(s) === i; });
    return place({ name: parts.join(', '), latitude: r && r.latitude, longitude: r && r.longitude });
  }).filter(Boolean).slice(0, 8);
}

// What a forecast answer becomes: now (the temperature, the weather's
// code, day or night), today (the day's code, its low and its high, the
// chance of rain in percent where the source gives one), the next 24 hours
// from the next whole hour at the place (each its hour, temperature, code
// and day or night) and the week (each day its weekday, code, low and
// high). The codes are the WMO's, as the source hands them on. Null where
// the answer is no forecast.
function reading(body, where, degrees, nowMs) {
  let json;
  try { json = JSON.parse(String(body)); } catch (e) { return null; }
  const daily = (json && json.daily) || {};
  const hourly = (json && json.hourly) || {};
  const current = (json && json.current) || {};
  const at = function (list, i) { return Array.isArray(list) ? list[i] : undefined; };
  // The source's times are the place's own; its offset from UTC puts now in them.
  const localNow = nowMs + (number(json && json.utc_offset_seconds, -50400, 50400) || 0) * 1000;
  const hours = [];
  for (let i = 0; Array.isArray(hourly.time) && i < hourly.time.length && hours.length < 24; i++) {
    const when = Date.parse(String(hourly.time[i]) + ':00Z');
    const temp = number(at(hourly.temperature_2m, i), -150, 150);
    const code = number(at(hourly.weather_code, i), 0, 99);
    if (!Number.isFinite(when) || when < localNow || temp === null || code === null) continue;
    hours.push({ hour: parseInt(String(hourly.time[i]).slice(11, 13), 10) || 0, temp: temp, code: code, day: Number(at(hourly.is_day, i)) !== 0 });
  }
  const days = [];
  for (let i = 0; Array.isArray(daily.time) && i < daily.time.length && days.length < 7; i++) {
    const when = Date.parse(String(daily.time[i]) + 'T00:00:00Z');
    const low = number(at(daily.temperature_2m_min, i), -150, 150);
    const high = number(at(daily.temperature_2m_max, i), -150, 150);
    const code = number(at(daily.weather_code, i), 0, 99);
    if (!Number.isFinite(when) || low === null || high === null || code === null) continue;
    days.push({ weekday: new Date(when).getUTCDay(), code: code, low: low, high: high });
  }
  const first = function (list) { return Array.isArray(list) ? list[0] : undefined; };
  const high = number(first(daily.temperature_2m_max), -150, 150);
  const low = number(first(daily.temperature_2m_min), -150, 150);
  const today = number(first(daily.weather_code), 0, 99);
  if (high === null || low === null || today === null) return null;
  const code = number(current.weather_code, 0, 99);
  const rain = number(first(daily.precipitation_probability_max), 0, 100);
  return {
    place: where.name,
    unit: degrees,
    now: number(current.temperature_2m, -150, 150),
    code: code === null ? today : code,
    day: current.is_day !== 0,
    today: today,
    low: low,
    high: high,
    rain: rain === null ? null : Math.round(rain),
    at: Math.floor(nowMs / 1000),
    hours: hours,
    days: days,
    source: SOURCE.name
  };
}

// The line the channel carries to every display: the reading, or that
// there is none to show.
function message(read) {
  return read ? Object.assign({ kind: 'weather' }, read) : { kind: 'weather', off: true };
}

// The keeper: the place and the unit in a file of their own, the last
// reading beside them, a forecast asked for on a timer while a place is
// set, and every change handed to `push` as the channel's line.
// options: `file`, `get(url, { timeout, limit, headers })` resolving
// `{ body }`, `push(message)`, `logger`, `version` (the plugin's, for the
// user agent); for tests `now`, `later(fn, ms)`, `never(handle)` and
// `local(ms)` (the player's local hour and date of an instant).
class Forecast {
  constructor(options) {
    this.file = options.file;
    this.get = options.get;
    this.push = options.push || function () {};
    this.logger = options.logger || { info() {}, warn() {} };
    this.now = options.now || Date.now;
    this.later = options.later || function (fn, ms) { const t = setTimeout(fn, ms); if (t.unref) t.unref(); return t; };
    this.never = options.never || clearTimeout;
    this.local = options.local || null;
    this.agent = agent(options.version);
    this.place = null;
    this.unit = 'C';
    this.read = null;
    this.error = '';
    // What the first source said when the second served the reading on
    // show; empty while the first serves.
    this.firstError = '';
    this.timer = null;
    this.running = false;
    this.asking = null;
  }

  // One request, the player identifying itself.
  ask(url) {
    return this.get(url, { timeout: TIMEOUT_MS, limit: LIMIT_BYTES, headers: { 'user-agent': this.agent } });
  }

  load() {
    let kept = null;
    try { kept = JSON.parse(fs.readFileSync(this.file, 'utf8')); } catch (e) { /* none kept */ }
    this.place = place(kept && kept.place);
    this.unit = unit(kept && kept.unit);
    const read = kept && kept.reading;
    this.read = this.place && read && typeof read === 'object' && read.place === this.place.name && read.unit === this.unit && Number.isFinite(read.at) ? read : null;
  }

  // The source that served the reading on show: the first unless the
  // reading says the second.
  served() {
    const read = this.current();
    const name = read && read.source;
    return SOURCES.find(function (s) { return s.name === name; }) || SOURCE;
  }

  save() {
    try {
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      const tmp = this.file + '.part';
      fs.writeFileSync(tmp, JSON.stringify({ place: this.place, unit: this.unit, reading: this.read }, null, 2));
      fs.renameSync(tmp, this.file);
    } catch (e) {
      this.logger.warn('weather: not kept: ' + e.message);
    }
  }

  // The reading while it is fresh enough to show, else null.
  current() {
    return this.read && this.now() - this.read.at * 1000 <= STALE_MS ? this.read : null;
  }

  state() {
    return { place: this.place, unit: this.unit, reading: this.current(), error: this.error, firstError: this.firstError, source: SOURCE, served: this.served(), sources: SOURCES };
  }

  // The plugin starts: what was kept is read, shown if still fresh, and a
  // forecast asked for at once when a place is set.
  start() {
    this.running = true;
    this.load();
    this.push(message(this.current()));
    if (this.place) return this.refresh();
    return Promise.resolve();
  }

  stop() {
    this.running = false;
    if (this.timer) { this.never(this.timer); this.timer = null; }
  }

  // A place chosen, the unit changed, or the forecast turned off (no
  // place). Null for a body that is neither.
  set(body) {
    body = body || {};
    const off = body.place === null || body.off === true;
    const where = off ? null : (body.place !== undefined ? place(body.place) : this.place);
    if (!off && body.place !== undefined && !where) return null;
    const degrees = body.unit !== undefined ? unit(body.unit) : this.unit;
    const same = !!where === !!this.place && (!where || (where.name === this.place.name && where.latitude === this.place.latitude && where.longitude === this.place.longitude)) && degrees === this.unit;
    this.place = where;
    this.unit = degrees;
    if (!same) { this.read = null; this.error = ''; this.firstError = ''; }
    this.save();
    if (!where) {
      if (this.timer) { this.never(this.timer); this.timer = null; }
      this.push(message(null));
      return Promise.resolve(this.state());
    }
    const self = this;
    return this.refresh().then(function () { return self.state(); });
  }

  schedule(ms) {
    const self = this;
    if (this.timer) this.never(this.timer);
    this.timer = this.running && this.place ? this.later(function () { self.timer = null; self.refresh(); }, ms) : null;
  }

  // One forecast asked for and taken: Open-Meteo first; when it does not
  // answer, or answers with no forecast, MET Norway in the same breath,
  // the reading saying which served. A failure of both keeps the last
  // reading while it is fresh and asks again sooner. Never rejects.
  refresh() {
    const self = this;
    if (!this.place) return Promise.resolve();
    if (this.asking) return this.asking;
    const where = this.place;
    const degrees = this.unit;
    const why = function (e) { return String((e && (e.code || e.message)) || e).slice(0, 120); };
    let firstError = '';
    this.asking = Promise.resolve()
      .then(function () { return self.ask(forecastUrl(where, degrees)); })
      .then(function (answer) {
        const read = reading(answer && answer.body, where, degrees, self.now());
        if (!read) throw new Error('not a forecast');
        return read;
      })
      .catch(function (e) {
        firstError = why(e);
        self.logger.warn('weather: ' + SOURCE.name + ' did not answer (' + firstError + '); asking ' + metno.SOURCE.name);
        return Promise.resolve()
          .then(function () { return self.ask(metno.forecastUrl(where)); })
          .then(function (answer) {
            const read = metno.reading(answer && answer.body, where, degrees, self.now(), self.local);
            if (!read) throw new Error('not a forecast');
            return read;
          });
      })
      .then(function (read) {
        self.asking = null;
        // The place or the unit changed while this was on its way: not this one's to keep.
        if (self.place !== where || self.unit !== degrees) return;
        self.read = read;
        self.error = '';
        self.firstError = read.source === SOURCE.name ? '' : firstError;
        if (self.firstError) self.logger.info('weather: the forecast from ' + read.source);
        self.save();
        self.push(message(read));
        self.schedule(EVERY_MS);
      }, function (e) {
        self.asking = null;
        if (self.place !== where || self.unit !== degrees) return;
        self.error = (firstError ? SOURCE.name + ': ' + firstError + '; ' + metno.SOURCE.name + ': ' : '') + why(e);
        self.error = self.error.slice(0, 240);
        self.logger.warn('weather: no forecast: ' + self.error);
        self.push(message(self.current()));
        self.schedule(RETRY_MS);
      });
    return this.asking;
  }

  // Places of a name, for the user to choose one: Open-Meteo's search,
  // Nominatim's when it does not answer. Rejects with the second's error
  // when neither does.
  search(name, language) {
    const self = this;
    const wanted = String(name || '').trim();
    if (wanted.length < 2) return Promise.resolve([]);
    return Promise.resolve()
      .then(function () { return self.ask(searchUrl(wanted, language)); })
      .then(function (answer) { return found(answer && answer.body); })
      .catch(function (e) {
        self.logger.warn('weather: the place search of ' + SOURCE.name + ' did not answer (' + String((e && (e.code || e.message)) || e).slice(0, 120) + '); asking Nominatim');
        return Promise.resolve()
          .then(function () { return self.ask(secondSearchUrl(wanted, language)); })
          .then(function (answer) { return foundSecond(answer && answer.body); });
      });
  }
}

module.exports = {
  Forecast: Forecast, SOURCE: SOURCE, SOURCES: SOURCES, EVERY_MS: EVERY_MS, RETRY_MS: RETRY_MS, STALE_MS: STALE_MS,
  place: place, unit: unit, agent: agent, forecastUrl: forecastUrl, searchUrl: searchUrl, secondSearchUrl: secondSearchUrl,
  found: found, foundSecond: foundSecond, reading: reading, message: message
};
