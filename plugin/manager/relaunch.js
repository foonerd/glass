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

module.exports = { LAUNCH_MS: LAUNCH_MS, LONGEST_MS: LONGEST_MS, deaths: deaths, wait: wait, holdUntil: holdUntil };
