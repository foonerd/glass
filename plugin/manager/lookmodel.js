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
    'clock.face': 'type', 'clock.dial': 'station', 'clock.unlit': '0.08', 'clock.hands': 'ink', 'clock.marks': 'ink', 'clock.second': 'accent', 'clock.disc': 'style', 'clock.card': '#17171a',
    'date.show': 'off', 'date.place': 'top', 'date.format': '%A %-d %B', 'date.ink': 'ink', 'date.opacity': '0.86', 'date.glass': '0.55', 'date.tint': 'tint',
    'measure.bar': '72', 'measure.clock': '144', 'measure.date': '40',
    // The picture behind the clock and the date when nothing plays (none: the theme), and how far it is darkened.
    'idle.picture': '', 'idle.dim': '0.25',
    // Minutes with nothing playing and no touch after which the screen goes black; 0 for never.
    'idle.off': '0',
    // Milliseconds the screen takes to go black and to come back.
    'idle.fade': '500',
    // The clock on the idle screen's grid (glass-evo 0.1.39): the cells it
    // occupies and where it stands inside them; an empty place is the
    // arrangement before the grid.
    'clock.place': '', 'clock.align': 'centre middle', 'clock.margin': '20',
    'date.align': 'centre middle', 'date.margin': '20',
    // The forecast (glass-evo 0.1.44): on the grid and only, across the
    // bottom row unless the look says, drawn once a place is chosen.
    'weather.show': 'on', 'weather.span': 'today', 'weather.place': 'bottom left-right', 'weather.align': 'centre middle', 'weather.margin': '20',
    'measure.weather': '40', 'weather.ink': 'ink', 'weather.opacity': '0.86', 'weather.glass': '0.55', 'weather.tint': 'tint'
  };
  var KEYS = Object.keys(BUILTIN);
  var NUMBERS = /^(glass\.(bar|sheet|hairline)|(buttons|clock|date)\.(opacity|glass)|clock\.unlit|measure\.[a-z]+|idle\.dim|idle\.off|idle\.fade)$/;
  // The clock's faces, set in type or drawn, and a dial's styles, as
  // glass-evo names them (from its 0.1.15).
  var CLOCK_FACES = ['type', 'seven', 'sixteen', 'flip', 'dial'];
  var DIAL_STYLES = ['station', 'numbers', 'roman', 'plain'];
  // The face a look's clock has: the one it names, `type` for a word that is none.
  function clockFace(v) {
    var face = String((v || {})['clock.face'] || '').trim().toLowerCase();
    return CLOCK_FACES.indexOf(face) === -1 ? 'type' : face;
  }
  function dialStyle(v) {
    var style = String((v || {})['clock.dial'] || '').trim().toLowerCase();
    return DIAL_STYLES.indexOf(style) === -1 ? 'station' : style;
  }
  // Taken to the letter: a pattern, and a picture's file name.
  var PATTERNS = /\.format$|^idle\.picture$/;

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
  // what the theme on show brings, the user's adjustments.
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
  // user's differs from what stands beneath it (`base`: the look, and what
  // the theme on show brings over it) and null, to be removed, where it
  // does not. Nothing of a look is ever written among the settings, so a
  // look that changes later shows as its author changed it. A key that is
  // `stored` with the value it has now is not named and stays as it is: an
  // adjustment the theme on show happens to agree with is still there for
  // a theme that does not.
  function changes(base, values, stored) {
    var kept = known(stored);
    var set = {};
    KEYS.forEach(function (key) {
      if (kept[key] !== undefined && same(key, kept[key], values[key])) return;
      set[key] = same(key, base[key], values[key]) ? null : values[key];
    });
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

  // The idle screen's grid, as the face reads it: a place names the cells
  // an element occupies, rows then columns, each a name or a range
  // (`middle left-centre`); null for anything else, an empty place among it.
  var GRID_ROWS = ['top', 'middle', 'bottom'];
  var GRID_COLUMNS = ['left', 'centre', 'right'];
  function cells(text) {
    var words = String(text || '').trim().split(/\s+/);
    if (words.length !== 2) return null;
    var range = function (word, names) {
      var ends = word.toLowerCase().replace('center', 'centre').split('-');
      if (ends.length > 2) return null;
      var a = names.indexOf(ends[0]), b = names.indexOf(ends[ends.length - 1]);
      return a === -1 || b === -1 ? null : [Math.min(a, b), Math.max(a, b)];
    };
    var rows = range(words[0], GRID_ROWS), columns = range(words[1], GRID_COLUMNS);
    return rows && columns ? { r0: rows[0], r1: rows[1], c0: columns[0], c1: columns[1] } : null;
  }
  // The place that names a block of cells.
  function cellsText(on) {
    var span = function (a, b, names) { return a === b ? names[a] : names[a] + '-' + names[b]; };
    return span(on.r0, on.r1, GRID_ROWS) + ' ' + span(on.c0, on.c1, GRID_COLUMNS);
  }
  // Where an element stands inside its cells: across and down, the middle unless said.
  function align(text) {
    var at = { across: 'centre', down: 'middle' };
    var words = String(text || '').trim().split(/\s+/).filter(Boolean);
    for (var i = 0; i < words.length; i++) {
      var w = words[i].toLowerCase().replace('center', 'centre');
      if (GRID_COLUMNS.indexOf(w) !== -1) at.across = w;
      else if (GRID_ROWS.indexOf(w) !== -1) at.down = w;
      else return null;
    }
    return at;
  }
  // A side of an alignment as a flex container's word for it.
  var FLEXED = { left: 'flex-start', centre: 'center', right: 'flex-end', top: 'flex-start', middle: 'center', bottom: 'flex-end' };
  function flexed(side) { return FLEXED[side] || 'center'; }
  // The side an element is set against: the one named, or, where the
  // element is larger than its cells less the margin (`over`), the far
  // one, so that its excess runs over the side named and "top" always
  // moves it up and "left" always left, as the face sets it. The middle
  // stays the middle.
  var FAR = { left: 'right', right: 'left', top: 'bottom', bottom: 'top' };
  function turned(side, over) { return over && FAR[side] ? FAR[side] : side; }

  // What the forecast shows, as one of its six words; today unless said.
  var SPANS = ['today', 'hours2', 'hours3', 'hours4', 'hours6', 'week'];
  function span(v) {
    var word = String(v['weather.span'] || '').trim().toLowerCase();
    return SPANS.indexOf(word) === -1 ? 'today' : word;
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
  // The measures the face lays the idle screen out with, in units: the
  // screen's margin, the room a glass keeps about its words (sideways,
  // above and below), the least of that room, and the gap between a clock
  // and a date.
  var IDLE = { margin: 20, pad: [28, 12], least: [6, 4], gap: 8 };
  // The most room the face gives a line whose size the look comes with (a
  // size the user set is drawn as set, whatever it runs over), on a
  // picture of `width` by `height` whose unit is `unit` long: the date no
  // wider than the picture leaves beside its glass and the margins and no
  // taller than a quarter of the picture; the clock in the whole width and
  // in what the bar (`below`) and the date (`dateHeight`, at the `top`
  // with its glass and margins, or beside the clock with the gap) leave of
  // the height, less the least room a glass keeps.
  function idleMost(width, height, unit, below, dateHeight, place) {
    var taken = !dateHeight ? 0 : place === 'top' ? dateHeight + (2 * IDLE.margin + 2 * IDLE.pad[1]) * unit : dateHeight + IDLE.gap * unit;
    return {
      date: [Math.max(0, width - 2 * (IDLE.margin + IDLE.pad[0]) * unit), height / 4],
      clock: [Math.max(0, width - 2 * IDLE.least[0] * unit), Math.max(0, height - below - taken - 2 * IDLE.least[1] * unit)]
    };
  }
  // The room a glass keeps about words `words` long in a space `space`
  // long: the room designed while that much is left on either side, less
  // as the words take more of the space, never under the least.
  function padAbout(space, words, designed, least) {
    return Math.min(designed, Math.max(Math.min(least, designed), Math.max(0, space - words) / 2));
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
    CLOCK_FACES: CLOCK_FACES, DIAL_STYLES: DIAL_STYLES, clockFace: clockFace, dialStyle: dialStyle,
    known: known, lay: lay, same: same, differs: differs, changes: changes,
    isOn: isOn, colourOf: colourOf, rgb: rgb, share: share, frostOn: frostOn, backgrounds: backgrounds,
    cells: cells, cellsText: cellsText, align: align, flexed: flexed, turned: turned, SPANS: SPANS, span: span, GRID_ROWS: GRID_ROWS, GRID_COLUMNS: GRID_COLUMNS,
    solidWord: solidWord, sizeOf: sizeOf, sizeWord: sizeWord, unitsOf: unitsOf, fitted: fitted, idleMost: idleMost, padAbout: padAbout, IDLE: IDLE,
    sample: sample, strftime: strftime
  };
});
