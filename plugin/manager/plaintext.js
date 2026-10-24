'use strict';
// Names as the player's sources send them, made plain for what Glass
// draws. A few sources write web markup into their names for Volumio's
// classic interface to render: a badge in a span before a track's name, an
// icon or a picture before a list's title, a colour on the chosen option,
// entities for ampersands and quotes. Glass draws text on every surface,
// so every name passes here once, where it enters the channel: tags gone,
// icons and pictures with them, entities decoded, spaces tidied, and a
// badge in front of a name taken off it and kept as a word of its own.

// The entities a name is likely to carry; a numeric one decodes by its number.
const ENTITIES = {
  amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', ndash: '–', mdash: '—',
  hellip: '…', copy: '©', reg: '®', trade: '™', laquo: '«', raquo: '»',
  lsquo: '‘', rsquo: '’', ldquo: '“', rdquo: '”', middot: '·', bull: '•',
  eacute: 'é', egrave: 'è', ecirc: 'ê', agrave: 'à', aacute: 'á', acirc: 'â',
  uuml: 'ü', ouml: 'ö', auml: 'ä', szlig: 'ß', ccedil: 'ç', ntilde: 'ñ',
  iacute: 'í', oacute: 'ó', uacute: 'ú', ocirc: 'ô', ucirc: 'û', icirc: 'î'
};

// The badge words one family of sources puts before a name, in a span of
// their own: what the item is. Known so the word can go and the kind stay.
const BADGES = { TRACK: 'track', ALBUM: 'album', ARTIST: 'artist', LABEL: 'label', 'NON-PLAYABLE': 'non-playable' };
// The services whose badge word may arrive glued to a name with its span
// already gone ("TRACKMake It Better"): only there is a leading badge word
// taken off a plain name, and only before a capitalised word.
const GLUED_SERVICES = new Set(['bandcamp']);

// How many names carried markup or a badge since the backend started, and
// from which services: the Status tab's fact and Find a problem's note.
const tally = { count: 0, services: [], at: null };

function tallied(service) {
  tally.count += 1;
  tally.at = new Date().toISOString();
  const name = service === undefined || service === null ? '' : String(service).trim();
  if (name && tally.services.indexOf(name) === -1) tally.services.push(name);
}

function markupFact() {
  return { count: tally.count, services: tally.services.slice(), at: tally.at };
}

function resetTally() {
  tally.count = 0;
  tally.services = [];
  tally.at = null;
}

function decodeEntities(s) {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, body) => {
    if (body[0] === '#') {
      const hex = body[1] === 'x' || body[1] === 'X';
      const n = parseInt(hex ? body.slice(2) : body.slice(1), hex ? 16 : 10);
      return Number.isFinite(n) && n > 0 && n <= 0x10ffff ? String.fromCodePoint(n) : whole;
    }
    const v = ENTITIES[body.toLowerCase()];
    return v === undefined ? whole : v;
  });
}

// Whether a name carries any markup or entity at all: the quick way out
// for the many that do not.
function marked(s) {
  return s.indexOf('<') !== -1 || s.indexOf('&') !== -1;
}

// A name made plain: `{ text, badge, marked }`, the badge the word a
// leading span of capitals carried (lowercase, '' for none), `marked`
// whether anything had to go. `service` names the source, for the glued
// case.
function name(value, service) {
  let s = value === undefined || value === null ? '' : String(value);
  let badge = '';
  const had = marked(s);
  if (had) {
    // A badge: a span at the head holding one word of capitals, and nothing else.
    const lead = /^\s*<span\b[^>]*>\s*([A-Z][A-Z-]{1,14})\s*<\/span>\s*/.exec(s);
    if (lead && BADGES[lead[1]]) {
      badge = BADGES[lead[1]];
      s = s.slice(lead[0].length);
    }
    s = s.replace(/<[^>]*>/g, ' ');
    s = decodeEntities(s);
  }
  s = s.replace(/\s+/g, ' ').trim();
  if (!badge && service && GLUED_SERVICES.has(String(service).toLowerCase())) {
    const glued = /^(TRACK|ALBUM|ARTIST|LABEL)(?=[A-Z][a-z])/.exec(s);
    if (glued) {
      badge = BADGES[glued[1]];
      s = s.slice(glued[1].length).trim();
    }
  }
  if (had || badge) tallied(service);
  return { text: s, badge: badge, marked: had };
}

// A name as text alone.
function plain(value, service) {
  return name(value, service).text;
}

// The icon a badge stands for, Lucide's name; '' where it stands for none.
function badgeIcon(badge) {
  return { track: 'music', album: 'disc-3', artist: 'user-round', label: 'tag' }[badge] || '';
}

// A player's state with its title, artist and album made plain, on a copy;
// `cleared` counts the fields that carried markup. The state's other
// fields, the location and the cover among them, are as the player sent them.
function state(given) {
  if (!given || typeof given !== 'object') return { state: given, cleared: 0 };
  const out = Object.assign({}, given);
  let cleared = 0;
  ['title', 'artist', 'album'].forEach((field) => {
    const v = given[field];
    if (typeof v !== 'string' || !v) return;
    const made = name(v, given.service);
    if (made.marked || made.badge) cleared += 1;
    out[field] = made.text;
  });
  return { state: out, cleared: cleared };
}

module.exports = { name, plain, state, badgeIcon, decodeEntities, markupFact, resetTally, BADGES };
