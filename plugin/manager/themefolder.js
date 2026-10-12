'use strict';
// The theme the user chose against the folder the engine's configuration
// names. After a plugin update Volumio replaces the plugin's folder, and the
// engine's meter.txt in it comes back from the template, naming the
// template's own folder: the chosen theme is put back at the plugin's start
// when the two differ, instead of waiting for the player's next state.

function text(value) {
  return typeof value === 'string' ? value.trim() : '';
}

// Whether the chosen folder must be put back: a chosen folder that is a
// plain name, and a configuration naming something else (or nothing).
function wanted(named, chosen) {
  var want = text(chosen);
  if (!want || want.indexOf('/') !== -1 || want.indexOf('..') !== -1) return false;
  return text(named) !== want;
}

module.exports = { wanted };
