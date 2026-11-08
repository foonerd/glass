'use strict';
// The pictures in turn when nothing plays: what the look asks of the
// slideshow (`face.idle.slide` and its lines), which picture comes next,
// and the picture line the player tells the display, the browser faces
// and the remotes. Pure: the plugin keeps the clock and does the telling.

const EVERY_MIN = 10;
const EVERY_LEAST = 1;
const EVERY_MOST = 240;
const MS_DEFAULT = 500;
const MS_MOST = 10000;
const TRANSITIONS = ['none', 'fade', 'merge'];

function on(value) {
  const v = String(value === undefined || value === null ? '' : value).trim().toLowerCase();
  return v === 'on' || v === 'true' || v === '1' || v === 'yes';
}

function bounded(value, least, most, fallback) {
  const n = parseInt(value, 10);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(most, Math.max(least, n));
}

// What the look asks, from the meter configuration's face keys, the
// pictures the player has (as the list the Manager serves, with stamps)
// and whether the board may move a picture (the performance profile's
// word, as for the movie): the slideshow is on when the switch is, a
// picture is chosen and the folder holds more than one.
function settings(current, pictures, motionAllowed) {
  current = current || {};
  const names = (pictures || []).map(function (p) { return typeof p === 'string' ? p : (p && p.name) || ''; }).filter(Boolean);
  const chosen = String(current['face.idle.picture'] || '').trim();
  const transition = String(current['face.idle.slide.transition'] || 'fade').trim().toLowerCase();
  return {
    on: on(current['face.idle.slide']) && !!chosen && names.length > 1,
    names: names,
    chosen: chosen,
    every: bounded(current['face.idle.slide.every'], EVERY_LEAST, EVERY_MOST, EVERY_MIN),
    random: on(current['face.idle.slide.random']),
    transition: TRANSITIONS.indexOf(transition) === -1 ? 'fade' : transition,
    ms: bounded(current['face.idle.slide.ms'], 0, MS_MOST, MS_DEFAULT),
    motion: on(current['face.idle.slide.motion']) && motionAllowed !== false
  };
}

// The picture after `current` among `names`: the next in the folder's
// order, wrapping, or a random one that is never the same twice in a row.
// A current name not in the list starts at the first.
function next(names, current, random, rnd) {
  const list = (names || []).filter(Boolean);
  if (!list.length) return '';
  if (list.length === 1) return list[0];
  const at = list.indexOf(current);
  if (random) {
    const roll = typeof rnd === 'function' ? rnd : Math.random;
    const others = list.filter(function (n) { return n !== current; });
    return others[Math.min(others.length - 1, Math.floor(roll() * others.length))];
  }
  return list[(at + 1) % list.length];
}

// The line the player tells: the picture's name, its address with the
// file's stamp for a face that is brought its pictures, the change to it
// and its length, the motion, and how long the picture shows (the motion's
// span). An empty name hands the choice back to the look.
function line(name, stamp, s) {
  if (!name) return { kind: 'picture', name: '' };
  return {
    kind: 'picture',
    name: name,
    url: '/api/backgrounds/' + encodeURIComponent(name) + '/file?v=' + (stamp || 0),
    stamp: stamp || 0,
    transition: s.transition,
    ms: s.transition === 'none' ? 0 : s.ms,
    motion: !!s.motion,
    span: s.every * 60000
  };
}

module.exports = { settings, next, line, EVERY_MIN, MS_DEFAULT, TRANSITIONS };
