'use strict';

// The language of Glass's own words: the Manager, Anymote and the face's
// dates. The player's language (Volumio's setting) unless one is set in
// Glass, and only a language Glass ships; a code with a region (pt_BR)
// falls to its language where that is shipped, and English stands behind
// everything. The plugin's own settings page inside Volumio keeps
// Volumio's language, which is Volumio's to translate.

const fs = require('fs');

// The languages a strings folder holds, by code, sorted.
function shipped(dir) {
  let names = [];
  try { names = fs.readdirSync(dir); } catch (e) { return []; }
  return names
    .map(function (n) { return /^strings_([a-z]{2,5})\.json$/.exec(n); })
    .filter(Boolean)
    .map(function (m) { return m[1]; })
    .sort();
}

// A code as written anywhere, down to letters, an underscore or a dash.
function clean(code) {
  return String(code || '').trim().toLowerCase().replace(/[^a-z_-]/g, '').slice(0, 8);
}

// The shipped language a code names: itself, else its language before a
// region, else none.
function resolve(code, languages) {
  const c = clean(code);
  if (!c) return '';
  if (languages.indexOf(c) !== -1) return c;
  const base = c.split(/[_-]/)[0];
  if (base && languages.indexOf(base) !== -1) return base;
  return '';
}

// The language in force: the one set in Glass where it is shipped, else
// the player's where that is, else English.
function pick(set, player, languages) {
  return resolve(set, languages) || resolve(player, languages) || 'en';
}

module.exports = { shipped, clean, resolve, pick };
