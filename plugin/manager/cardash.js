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

module.exports = { TIME, minutesOf, periodAt, nextSwitch };
