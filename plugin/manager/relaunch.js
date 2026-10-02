'use strict';
// When a display that died is started again. A display that dies at launch
// (a library it needs is missing, there is no screen to open) dies the same
// way the next time: started again at the screensaver's cadence it wrote
// an error and a warning into the player's log every time, every second
// where the screensaver's delay is a second, for as long as music played.
// Each death at launch in a row doubles the wait, up to a minute; a display
// that ran, or left cleanly, starts the count again.

// A display that dies sooner than this after its launch died at launch.
const LAUNCH_MS = 10000;
// The longest wait between two starts, unless the screensaver's own delay is longer.
const LONGEST_MS = 60000;

// The deaths at launch in a row after one more exit.
function deaths(count, exit) {
  return exit.clean || exit.ranMs >= LAUNCH_MS ? 0 : count + 1;
}

// The wait before the next start after `count` deaths at launch in a row:
// the screensaver's delay, a second at least, doubled for each death after
// the first.
function wait(count, baseMs) {
  const base = Math.max(1000, Number(baseMs) || 0);
  if (count <= 1) return base;
  return Math.max(base, Math.min(LONGEST_MS, base * Math.pow(2, Math.min(count - 1, 16))));
}

// Until when no start is made, for a death at `now`: the starts come from
// timers of the screensaver's cadence, so the hold ends half a cadence
// before the wait does and the timer's next turn after it is the start.
function holdUntil(now, count, baseMs) {
  const base = Math.max(1000, Number(baseMs) || 0);
  return now + wait(count, baseMs) - base / 2;
}

// What the waits are counted from. Over a kiosk the display is a screensaver,
// started at the screensaver's delay, and that delay is the base. On a screen
// that is the display's own it is started by the screen's watcher as soon as
// it is found gone, and the screensaver's delay has no part in it: the base
// is a second, so a display that cannot hold the screen is known in seconds,
// and where glass-evo was handed the screen, the kiosk has it back in seconds.
const OWN_SCREEN_MS = 1000;
function base(ownScreen, screensaverMs) {
  return ownScreen ? OWN_SCREEN_MS : Math.max(1000, Number(screensaverMs) || 0);
}

// What a display that died at launch is started with the next time: the
// graphics libraries' own words switched on. Of a screen it cannot open SDL
// keeps only its last word ("Can't load EGL/GL library on window creation"),
// whichever step failed; Mesa, asked, names the library or the driver that
// would not load, and the journal then has it.
function diagnosing(deathsInARow) {
  return deathsInARow > 0 ? { EGL_LOG_LEVEL: 'debug', LIBGL_DEBUG: 'verbose' } : {};
}

// The libraries a display needs to draw on a screen of its own that the
// system's loader does not know, from `ldconfig -p`: EGL, the buffer manager,
// and OpenGL or OpenGL ES (SDL takes either). An unreadable list names none.
function missingLibraries(ldconfigText) {
  const text = String(ldconfigText || '');
  if (!/\.so/.test(text)) return [];
  const has = function (name) { return text.indexOf(name) !== -1; };
  const missing = [];
  if (!has('libEGL.so.1')) missing.push('libEGL.so.1');
  if (!has('libGL.so.1') && !has('libGLESv2.so.2')) missing.push('libGL.so.1');
  if (!has('libgbm.so.1')) missing.push('libgbm.so.1');
  return missing;
}

// Whether a display's last words speak of the graphics libraries.
function aboutGraphics(text) {
  return /\bEGL\b|GL library|\bGLES|\bgbm\b|MESA/i.test(String(text || ''));
}

// Why a display died, in one line: how it left and its own last word first,
// then what was said before that reads as a fault, each once, as far as
// `limit` characters go. The last word is never cut out by what came before.
const FAULT = /warning|error|fail|cannot|could not|unable|no such|not found|denied|missing/i;
function reason(exitWord, lines, limit) {
  const all = (lines || []).map(function (l) { return String(l).trim(); }).filter(Boolean);
  const last = all.length ? all[all.length - 1] : '';
  const max = Math.max(40, Number(limit) || 300);
  let out = [String(exitWord || '').trim(), last].filter(Boolean).join(' ').slice(0, max);
  const said = [last];
  all.slice(0, -1).forEach(function (line) {
    if (!FAULT.test(line) || said.indexOf(line) !== -1) return;
    said.push(line);
    if ((out + ' | ' + line).length <= max) out += ' | ' + line;
  });
  return out;
}

module.exports = { LAUNCH_MS: LAUNCH_MS, LONGEST_MS: LONGEST_MS, OWN_SCREEN_MS: OWN_SCREEN_MS, deaths: deaths, wait: wait, holdUntil: holdUntil, base: base, diagnosing: diagnosing, missingLibraries: missingLibraries, aboutGraphics: aboutGraphics, reason: reason };
