// The look panel's model: what the face shows for a look and the user's
// adjustments over it, what a save has to say, and the words and the
// likeness the page shows of it. Pure, and one script for the page and for
// its tests: the Manager's page loads it as GlassLook.
(function (root, make) {
  if (typeof module === 'object' && module.exports) module.exports = make();
  else root.GlassLook = make();
})(this, function () {
  'use strict';

  // What the panel sets, each with the built-in look's value. A glass-evo
  // that ships its built-in look written out is believed over these; they
  // stand for the ones before 0.1.8, which did not.
  var BUILTIN = {
    'colours.tint': 'artwork', 'colours.accent': 'artwork', 'colours.ink': '#f2f2f5',
    'glass.bar': '0.75', 'glass.sheet': '0.78', 'glass.hairline': '0.12', 'glass.frost': 'auto',
    'buttons.ink': 'ink', 'buttons.opacity': '1.0',
    'clock.show': 'on', 'clock.format': '%H:%M', 'clock.ink': 'ink', 'clock.opacity': '0.86', 'clock.glass': '0.55', 'clock.tint': 'tint',
    'date.show': 'off', 'date.place': 'top', 'date.format': '%A %-d %B', 'date.ink': 'ink', 'date.opacity': '0.86', 'date.glass': '0.55', 'date.tint': 'tint',
    'measure.bar': '72', 'measure.clock': '144', 'measure.date': '40'
  };
  var KEYS = Object.keys(BUILTIN);
  var NUMBERS = /^(glass\.(bar|sheet|hairline)|(buttons|clock|date)\.(opacity|glass)|measure\.[a-z]+)$/;
  var PATTERNS = /\.format$/;

  // The keys of a theme or of the settings that the panel knows, under the
  // names it uses: a face reads colors as colours.
  function known(keys) {
    var out = {};
    Object.keys(keys || {}).forEach(function (key) {
      var name = key.replace(/^colors\./, 'colours.');
      if (BUILTIN[name] !== undefined && String(keys[key]).trim() !== '') out[name] = String(keys[key]).trim();
    });
    return out;
  }

  // Layers, the later over the earlier: the built-in look, a look's keys,
  // the user's adjustments.
  function lay() {
    var out = {};
    KEYS.forEach(function (key) { out[key] = BUILTIN[key]; });
    Array.prototype.forEach.call(arguments, function (layer) {
      var keys = known(layer);
      Object.keys(keys).forEach(function (key) { out[key] = keys[key]; });
    });
    return out;
  }

  // Whether two values of a key say the same to the face.
  function same(key, a, b) {
    a = String(a); b = String(b);
    if (PATTERNS.test(key)) return a === b;
    if (NUMBERS.test(key)) {
      var x = parseFloat(a), y = parseFloat(b);
      return isNaN(x) || isNaN(y) ? a === b : Math.abs(x - y) < 0.005;
    }
    return a.toLowerCase() === b.toLowerCase();
  }

  function differs(a, b) {
    return KEYS.some(function (key) { return !same(key, a[key], b[key]); });
  }

  // What a save says: every key the panel sets, with its value where the
  // user's differs from the look's and null, to be removed, where it does
  // not. Nothing of the look itself is ever written among the settings, so
  // a look that changes later shows as its author changed it.
  function changes(base, values) {
    var set = {};
    KEYS.forEach(function (key) { set[key] = same(key, base[key], values[key]) ? null : values[key]; });
    return set;
  }

  function isOn(value) { return ['on', 'true', 'yes', '1'].indexOf(String(value).toLowerCase()) !== -1; }
  function isOff(value) { return ['off', 'false', 'no', '0'].indexOf(String(value).toLowerCase()) !== -1; }

  // A colour as the face reads one, or null for a word (artwork, ink, tint).
  function colourOf(value) {
    return /^#[0-9a-fA-F]{6}$/.test(String(value)) ? String(value).toLowerCase() : null;
  }

  function rgb(colour) {
    return [1, 3, 5].map(function (at) { return parseInt(colour.slice(at, at + 2), 16); }).join(',');
  }

  // A share between 0 and 1 from a value, the fallback where it is none.
  function share(value, fallback) {
    var n = parseFloat(value);
    return isNaN(n) ? fallback : Math.min(1, Math.max(0, n));
  }

  // Whether what lies behind a background is blurred: the user's or the
  // look's word, else as suits the board.
  function frostOn(values, suits) {
    var word = values['glass.frost'];
    return isOn(word) ? true : isOff(word) ? false : !!suits;
  }

  // Every background made as solid as the buttons' is asked to be, each
  // keeping the distance the look gave it from the buttons'; one that is
  // off stays off.
  function backgrounds(base, values, bar) {
    var from = share(base['glass.bar'], 0.75);
    var to = Math.min(1, Math.max(0.1, bar));
    var out = { 'glass.bar': to.toFixed(2) };
    ['glass.sheet', 'clock.glass', 'date.glass'].forEach(function (key) {
      if (share(values[key], 0) === 0) return;
      var step = share(base[key], from) - from;
      out[key] = Math.min(1, Math.max(0.1, to + step)).toFixed(2);
    });
    return out;
  }

  // How solid a background is, and how large a piece, as the word to say.
  function solidWord(value) {
    var s = share(value, 0);
    return s === 0 ? 'NONE' : s <= 0.15 ? 'FAINT' : s < 0.5 ? 'LIGHT' : s < 0.9 ? 'FAIR' : 'SOLID';
  }

  // A piece's size as a share of the built-in look's, in percent.
  function sizeOf(key, value) {
    var units = parseFloat(value);
    var designed = parseFloat(BUILTIN[key]);
    return isNaN(units) ? 100 : Math.round(units / designed * 100);
  }
  function sizeWord(key, value) {
    var size = sizeOf(key, value);
    return size < 90 ? 'SMALL' : size <= 110 ? 'STANDARD' : size <= 160 ? 'LARGE' : 'HUGE';
  }
  function unitsOf(key, percent) {
    return String(Math.round(parseFloat(BUILTIN[key]) * percent / 100));
  }

  // The size a line is set at so that it fits, as the face reckons it: the
  // size wanted, or less by as much as its room at that size exceeds the
  // room there is.
  function fitted(wanted, room, most) {
    if (room[0] <= most[0] && room[1] <= most[1]) return wanted;
    return wanted * Math.min(most[0] / Math.max(room[0], 1), most[1] / Math.max(room[1], 1));
  }
  // The most room the face gives the lines of the idle screen, on a picture
  // of `width` by `height` whose unit is `unit` long: no line wider than
  // the picture leaves beside its glass (28 units) and the margins (20);
  // the date no taller than a quarter of the picture; the clock in what
  // the bar (`below`) and the date (`dateHeight`, at the `top` with its
  // glass and margins, or beside the clock with the gap) leave of the
  // height, and never taller than half the picture.
  function idleMost(width, height, unit, below, dateHeight, place) {
    var widest = Math.max(0, width - 2 * (20 + 28) * unit);
    var taken = !dateHeight ? 0 : place === 'top' ? dateHeight + (20 + 2 * 12 + 20) * unit : dateHeight + 8 * unit;
    var tallest = Math.max(0, height - below - taken - 2 * (12 + 20) * unit);
    return { date: [widest, height / 4], clock: [widest, Math.min(tallest, height / 2)] };
  }

  // The patterns offered for the clock and the date; any other is typed.
  var CLOCK_PATTERNS = ['%H:%M', '%H:%M:%S', '%-I:%M %p', '%-I:%M:%S %p'];
  var DATE_PATTERNS = ['%A %-d %B', '%A, %B %-d', '%a %-d %b %Y', '%d/%m/%Y', '%m/%d/%Y', '%d.%m.%Y', '%Y-%m-%d'];
  // The moment the patterns are shown at: a Thursday afternoon whose every
  // number is short, so padding and its absence both show.
  function sample() { return new Date(2026, 9, 1, 13, 5, 9); }

  var DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  var MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];

  // A time in a pattern, as the face's strftime gives it: the directives a
  // clock and a date are written with, and the flags - (no padding),
  // _ (spaces), 0 (zeros) and ^ (capitals). One it does not know stands as
  // written.
  function strftime(pattern, at) {
    var hour12 = at.getHours() % 12 || 12;
    var numbers = {
      d: [at.getDate(), 2, '0'], e: [at.getDate(), 2, ' '], H: [at.getHours(), 2, '0'], I: [hour12, 2, '0'],
      k: [at.getHours(), 2, ' '], l: [hour12, 2, ' '], m: [at.getMonth() + 1, 2, '0'], M: [at.getMinutes(), 2, '0'],
      S: [at.getSeconds(), 2, '0'], y: [at.getFullYear() % 100, 2, '0'], Y: [at.getFullYear(), 1, '0'],
      j: [Math.round((new Date(at.getFullYear(), at.getMonth(), at.getDate()) - new Date(at.getFullYear(), 0, 1)) / 864e5) + 1, 3, '0']
    };
    var words = {
      a: DAYS[at.getDay()].slice(0, 3), A: DAYS[at.getDay()], b: MONTHS[at.getMonth()].slice(0, 3), h: MONTHS[at.getMonth()].slice(0, 3),
      B: MONTHS[at.getMonth()], p: at.getHours() < 12 ? 'AM' : 'PM', P: at.getHours() < 12 ? 'am' : 'pm'
    };
    var whole = { R: '%H:%M', T: '%H:%M:%S', D: '%m/%d/%y', F: '%Y-%m-%d' };
    return String(pattern).replace(/%([-_0^]?)([A-Za-z%])/g, function (all, flag, c) {
      if (c === '%') return '%';
      if (whole[c]) return strftime(whole[c], at);
      if (words[c] !== undefined) return flag === '^' ? words[c].toUpperCase() : words[c];
      if (!numbers[c]) return all;
      var text = String(numbers[c][0]);
      var fill = flag === '_' ? ' ' : flag === '0' ? '0' : numbers[c][2];
      while (flag !== '-' && text.length < numbers[c][1]) text = fill + text;
      return text;
    });
  }

  return {
    BUILTIN: BUILTIN, KEYS: KEYS, CLOCK_PATTERNS: CLOCK_PATTERNS, DATE_PATTERNS: DATE_PATTERNS,
    known: known, lay: lay, same: same, differs: differs, changes: changes,
    isOn: isOn, colourOf: colourOf, rgb: rgb, share: share, frostOn: frostOn, backgrounds: backgrounds,
    solidWord: solidWord, sizeOf: sizeOf, sizeWord: sizeWord, unitsOf: unitsOf, fitted: fitted, idleMost: idleMost,
    sample: sample, strftime: strftime
  };
});
