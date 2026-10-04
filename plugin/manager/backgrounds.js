'use strict';
// Pictures of the user's own for the screen when nothing plays: a folder
// on the player, reached through the Internal Storage share or by an
// upload from the Manager. glass-evo draws the chosen one behind its clock
// and date in the theme's place. Pure: what counts as a picture and what
// it may be called; the plugin keeps the folder.

const { kindOf } = require('./picture');

const MAX_BYTES = 32 * 1024 * 1024;
// How many bytes tell what a file is.
const TELLING = 12;
const ENDINGS = { 'image/jpeg': '.jpg', 'image/png': '.png', 'image/webp': '.webp' };
// A name the face's settings can carry as written: letters, digits, space
// and . _ ( ) -, with a picture's ending.
const NAME = /^[A-Za-z0-9][A-Za-z0-9 ._()-]{0,59}\.(jpe?g|png|webp)$/i;

// The ending a file's first bytes ask for, or null where it is no picture
// the face reads.
function endingOf(head) {
  return ENDINGS[kindOf(head)] || null;
}

// Whether a file in the folder is listed: named as above, nothing that walks.
function isName(name) {
  const n = String(name || '');
  return NAME.test(n) && n.indexOf('..') === -1;
}

// The name an uploaded picture is kept under: its own without folder and
// ending, what the settings cannot carry turned into hyphens, and the
// ending of what the file is.
function safeName(raw, ending) {
  const base = String(raw || '').replace(/^.*[\\/]/, '').replace(/\.[A-Za-z0-9]{1,5}$/, '')
    .replace(/[^A-Za-z0-9 ._()-]/g, '-').replace(/\.{2,}/g, '.').replace(/^[^A-Za-z0-9]+/, '').slice(0, 48).trim() || 'picture';
  const name = base + ending;
  return isName(name) ? name : null;
}

module.exports = { MAX_BYTES: MAX_BYTES, TELLING: TELLING, endingOf: endingOf, isName: isName, safeName: safeName };
