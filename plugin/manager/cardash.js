'use strict';

// Car Dash: a day theme and a night theme by the clock, switched by the
// plugin itself. Times are HH:MM in the player's own time zone.

const TIME = /^([01]\d|2[0-3]):([0-5]\d)$/;

// Minutes since midnight of an HH:MM text, or null when it is not one.
function minutesOf(text) {
  const m = TIME.exec(String(text || '').trim());
  return m ? parseInt(m[1], 10) * 60 + parseInt(m[2], 10) : null;
}

// 'day' or 'night' at `now` (a Date), when the day starts at `dayAt` and the
// night at `nightAt`; null when the times are not two different times.
function periodAt(now, dayAt, nightAt) {
  const d = minutesOf(dayAt);
  const n = minutesOf(nightAt);
  if (d === null || n === null || d === n) return null;
  const t = now.getHours() * 60 + now.getMinutes();
  const inDay = d < n ? (t >= d && t < n) : (t >= d || t < n);
  return inDay ? 'day' : 'night';
}

// The next switch after `now`: `{ at, period }`, the period that starts then.
function nextSwitch(now, dayAt, nightAt) {
  const d = minutesOf(dayAt);
  const n = minutesOf(nightAt);
  if (d === null || n === null || d === n) return null;
  const candidates = [[d, 'day'], [n, 'night']].map(function (pair) {
    const at = new Date(now.getFullYear(), now.getMonth(), now.getDate(), Math.floor(pair[0] / 60), pair[0] % 60, 0, 0);
    if (at.getTime() <= now.getTime()) at.setDate(at.getDate() + 1);
    return { at: at, period: pair[1] };
  });
  candidates.sort(function (a, b) { return a.at.getTime() - b.at.getTime(); });
  return candidates[0];
}

// ---- By the sun -----------------------------------------------------------

const ZONE_TABLES = ['/usr/share/zoneinfo/zone1970.tab', '/usr/share/zoneinfo/zone.tab'];

// A latitude and longitude from the zone table's ISO 6709 form,
// `+513030-0000731` (degrees, minutes and maybe seconds), or null.
function parseIso6709(text) {
  const m = /^([+-])(\d{2})(\d{2})(\d{2})?([+-])(\d{3})(\d{2})(\d{2})?$/.exec(String(text || '').trim());
  if (!m) return null;
  const lat = (parseInt(m[2], 10) + parseInt(m[3], 10) / 60 + (m[4] ? parseInt(m[4], 10) / 3600 : 0)) * (m[1] === '-' ? -1 : 1);
  const lon = (parseInt(m[6], 10) + parseInt(m[7], 10) / 60 + (m[8] ? parseInt(m[8], 10) / 3600 : 0)) * (m[5] === '-' ? -1 : 1);
  return { lat: Math.round(lat * 10000) / 10000, lon: Math.round(lon * 10000) / 10000 };
}

// The reference point of a time zone in a zone table's text, or null.
function zoneLocationIn(table, zone) {
  const lines = String(table || '').split(/\r?\n/);
  for (const line of lines) {
    if (!line || line.startsWith('#')) continue;
    const cols = line.split('\t');
    if (cols.length >= 3 && cols[2] === zone) {
      const at = parseIso6709(cols[1]);
      if (at) return Object.assign({ zone: zone }, at);
    }
  }
  return null;
}

// The player's time zone and its reference point from the system's zone
// table; the zone alone when the table has no line for it.
function zoneLocation(fs) {
  let zone = '';
  try { zone = Intl.DateTimeFormat().resolvedOptions().timeZone || ''; } catch (e) { zone = ''; }
  if (!zone) return null;
  for (const path of ZONE_TABLES) {
    try {
      const found = zoneLocationIn(fs.readFileSync(path, 'utf8'), zone);
      if (found) return found;
    } catch (e) { /* the next table, or none */ }
  }
  return { zone: zone, lat: null, lon: null };
}

// Sunrise and sunset on the calendar day of `date` (local) at a place, as
// Dates, or null on a day the sun does not rise or set there. NOAA's
// solar equations; within a minute or two of the almanac. `elevation`
// is the sun's altitude that counts as rising: -0.833 degrees for the
// horizon with refraction, -6 for civil twilight.
function sunTimes(date, lat, lon, elevation) {
  const rad = Math.PI / 180;
  const alt = elevation === undefined ? -0.833 : elevation;
  const y = date.getFullYear(), mo = date.getMonth(), d = date.getDate();
  const midnightUT = Date.UTC(y, mo, d);
  const jd = midnightUT / 86400000 + 2440587.5 + 0.5; // noon UT of that day
  const t = (jd - 2451545.0) / 36525.0;
  const L0 = ((280.46646 + t * (36000.76983 + t * 0.0003032)) % 360 + 360) % 360;
  const M = 357.52911 + t * (35999.05029 - 0.0001537 * t);
  const Mr = M * rad;
  const C = (1.914602 - t * (0.004817 + 0.000014 * t)) * Math.sin(Mr) + (0.019993 - 0.000101 * t) * Math.sin(2 * Mr) + 0.000289 * Math.sin(3 * Mr);
  const omega = 125.04 - 1934.136 * t;
  const lambda = L0 + C - 0.00569 - 0.00478 * Math.sin(omega * rad);
  const eps0 = 23 + (26 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60) / 60;
  const eps = eps0 + 0.00256 * Math.cos(omega * rad);
  const decl = Math.asin(Math.sin(eps * rad) * Math.sin(lambda * rad));
  const yy = Math.pow(Math.tan((eps / 2) * rad), 2);
  const e = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
  const L0r = L0 * rad;
  const eqt = 4 * (yy * Math.sin(2 * L0r) - 2 * e * Math.sin(Mr) + 4 * e * yy * Math.sin(Mr) * Math.cos(2 * L0r) - 0.5 * yy * yy * Math.sin(4 * L0r) - 1.25 * e * e * Math.sin(2 * Mr)) / rad;
  const latr = lat * rad;
  const cosHa = (Math.cos((90 - alt) * rad) - Math.sin(latr) * Math.sin(decl)) / (Math.cos(latr) * Math.cos(decl));
  if (!(cosHa >= -1 && cosHa <= 1)) return null;
  const ha = Math.acos(cosHa) / rad;
  const noonUT = 720 - 4 * lon - eqt;
  return { rise: new Date(midnightUT + (noonUT - ha * 4) * 60000), set: new Date(midnightUT + (noonUT + ha * 4) * 60000) };
}

// The day's switches at a place: the day from sunrise less `offsetMin`,
// the night from sunset plus it; null on a day without either.
function sunSwitches(date, lat, lon, offsetMin) {
  const sun = sunTimes(date, lat, lon);
  if (!sun) return null;
  const off = (offsetMin || 0) * 60000;
  return { dayAt: new Date(sun.rise.getTime() - off), nightAt: new Date(sun.set.getTime() + off), rise: sun.rise, set: sun.set };
}

// 'day' or 'night' at `now` by the sun, or null where the sun does not
// rise or set that day.
function periodBySun(now, lat, lon, offsetMin) {
  const today = sunSwitches(now, lat, lon, offsetMin);
  if (!today) return null;
  return now.getTime() >= today.dayAt.getTime() && now.getTime() < today.nightAt.getTime() ? 'day' : 'night';
}

// The next switch after `now` by the sun: `{ at, period }`, or null.
function nextSwitchBySun(now, lat, lon, offsetMin) {
  const candidates = [];
  for (let ahead = 0; ahead <= 2; ahead++) {
    const day = new Date(now.getFullYear(), now.getMonth(), now.getDate() + ahead, 12, 0, 0, 0);
    const s = sunSwitches(day, lat, lon, offsetMin);
    if (!s) continue;
    if (s.dayAt.getTime() > now.getTime()) candidates.push({ at: s.dayAt, period: 'day' });
    if (s.nightAt.getTime() > now.getTime()) candidates.push({ at: s.nightAt, period: 'night' });
  }
  candidates.sort(function (a, b) { return a.at.getTime() - b.at.getTime(); });
  return candidates[0] || null;
}

module.exports = { TIME, minutesOf, periodAt, nextSwitch, parseIso6709, zoneLocationIn, zoneLocation, sunTimes, sunSwitches, periodBySun, nextSwitchBySun };
