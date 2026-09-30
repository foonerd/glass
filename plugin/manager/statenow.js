'use strict';
// The player's state as it stands now. Volumio reports the position only
// when something happens, so a state kept since then is behind by the time
// since while the player plays; a Face, an Anymote page or a display that
// attaches later would count from that old position. A copy with the
// position moved on is what a replay hands out.

// `state` as Volumio pushed it (`seek` in milliseconds), `storedAt` the
// clock when it arrived, `now` the clock now. Not playing, no stamp, no
// number, or no time passed: the state as it is.
function advanced(state, storedAt, now) {
  if (!state || typeof state !== 'object') return state;
  if (state.status !== 'play' || typeof state.seek !== 'number' || !isFinite(state.seek)) return state;
  if (typeof storedAt !== 'number' || typeof now !== 'number') return state;
  const passed = Math.max(0, now - storedAt);
  if (!passed) return state;
  let seek = Math.round(state.seek + passed);
  const duration = Number(state.duration);
  if (isFinite(duration) && duration > 0) seek = Math.min(seek, Math.round(duration * 1000));
  return Object.assign({}, state, { seek });
}

module.exports = { advanced };
