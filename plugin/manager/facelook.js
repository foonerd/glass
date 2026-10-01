'use strict';

// The face's own settings in the display's configuration: the face.<name>
// keys the display hands to glass-evo unread. The Manager sets them, a
// theme's choice and the user's overrides over it, and says for the board
// whether frost over a moving theme suits it. Pure: the plans are made
// here, the plugin writes them.

const PREFIX = 'face.';
// A name is up to four words of letters and digits, dot between; a value
// is a short word, a number, a colour, or a pattern for a time or a date
// (%H:%M, %d/%m/%Y, %A, %B %-d). Nothing that could break out of a line of
// the configuration or make its writer quote the value.
const NAME = /^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*){0,3}$/;
const VALUE = /^[#%A-Za-z0-9_.:\/,() -]{1,64}$/;
// Set elsewhere: the size on the screen's own route, the board's word by
// the plugin at its start.
const OWN = ['size', 'frost.suits'];
// What a reset leaves: those, and nothing of the look.
const KEPT = OWN;

// The face's settings as the configuration's current section has them,
// by name.
function settingsOf(current) {
  const out = {};
  Object.keys(current || {}).forEach(function (key) {
    if (key.indexOf(PREFIX) === 0 && key.length > PREFIX.length) out[key.slice(PREFIX.length)] = String(current[key]);
  });
  return out;
}

// The changes a request asks for, as full keys with their values, null to
// remove; or the error of a name or a value that is not one.
function plan(current, set) {
  if (!set || typeof set !== 'object' || Array.isArray(set)) return { error: 'bad-request' };
  const changes = {};
  for (const name of Object.keys(set)) {
    if (!NAME.test(name) || OWN.indexOf(name) !== -1) return { error: 'bad-name', name };
    const value = set[name];
    const key = PREFIX + name;
    if (value === null || value === '') {
      if (current && current[key] !== undefined) changes[key] = null;
      continue;
    }
    const text = String(value).trim();
    if (!VALUE.test(text)) return { error: 'bad-value', name };
    if (!current || String(current[key]) !== text) changes[key] = text;
  }
  return { changes };
}

// A face theme's text as its keys, each under its section, the way the
// face reads it: `[glass]` and `bar = 0.6` is `glass.bar`. For the page to
// show what stands when the user has not said otherwise.
function themeKeys(text) {
  const found = {};
  let section = '';
  String(text || '').split('\n').forEach(function (raw) {
    const line = raw.trim();
    if (!line || line[0] === '#' || line[0] === ';') return;
    if (line[0] === '[' && line[line.length - 1] === ']') { section = line.slice(1, -1).trim().toLowerCase(); return; }
    const at = line.indexOf('=');
    if (at < 1) return;
    const key = line.slice(0, at).trim().toLowerCase();
    if (!key) return;
    found[section ? section + '.' + key : key] = line.slice(at + 1).trim();
  });
  return found;
}

// Back to the defaults: every key of the look removed.
function resetPlan(current) {
  const changes = {};
  Object.keys(settingsOf(current)).forEach(function (name) {
    if (KEPT.indexOf(name) === -1) changes[PREFIX + name] = null;
  });
  return { changes };
}

// A request with a reset: the look removed, then the request laid over
// what is left, so a key the reset removes and the request names again is
// set, whatever its value was before.
function planAll(current, set, reset) {
  const changes = reset ? resetPlan(current).changes : {};
  const left = Object.assign({}, current || {});
  Object.keys(changes).forEach(function (key) { delete left[key]; });
  const planned = plan(left, set || {});
  if (planned.error) return planned;
  Object.keys(planned.changes).forEach(function (key) { changes[key] = planned.changes[key]; });
  // Removed and set again to what it was: no change at all.
  Object.keys(changes).forEach(function (key) {
    if (changes[key] !== null && current && String(current[key]) === changes[key]) delete changes[key];
  });
  return { changes };
}

// Whether frost over a moving theme suits a board: where a frame has room
// for it. The user's switch decides either way.
function frostSuits(boardClass) {
  return ['pi4', 'pi5', 'x64'].indexOf(String(boardClass)) !== -1;
}

module.exports = { PREFIX, settingsOf, plan, planAll, resetPlan, frostSuits, themeKeys };
