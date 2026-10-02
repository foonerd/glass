'use strict';
// What the Face tab and Anymote show of the player: the theme alone, drawn
// by Glass's own module, or the theme with glass-evo's face over it, drawn
// by the module the glass-evo component carries. The user says which:
// `follow`, the face where glass-evo holds the player's own screen and the
// theme alone under the kiosk; `face`, the face wherever the component is
// here with its module; `theme`, the theme alone, as before there was a
// choice.

const MODES = ['follow', 'face', 'theme'];

// A mode from any spelling; `follow` for anything else.
function modeOf(value) {
  const v = String(value === undefined || value === null ? '' : value).trim().toLowerCase();
  return MODES.indexOf(v) === -1 ? 'follow' : v;
}

// Whether the views carry the face: `owner` is who holds the player's
// screen, `has` whether a glass-evo with a browser module is installed.
function carriesFace(mode, owner, has) {
  if (!has) return false;
  const m = modeOf(mode);
  if (m === 'theme') return false;
  if (m === 'face') return true;
  return owner === 'glass-evo';
}

// The name a face theme may have, as the face itself reads one: no folder
// above or below, nothing hidden.
function themeName(name) {
  const n = String(name === undefined || name === null ? '' : name).trim();
  return n && n[0] !== '.' && !/[/\\]/.test(n) ? n : null;
}

module.exports = { MODES: MODES, modeOf: modeOf, carriesFace: carriesFace, themeName: themeName };
