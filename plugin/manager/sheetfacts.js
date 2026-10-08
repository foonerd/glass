'use strict';
// The status sheet's facts of glass-evo, the remotes and a theme's own
// fonts, composed from what the Manager already holds: pure functions,
// facts in and plain values out, so the page sets them in words and the
// tests pin them. Nothing here reads the player; the server hands in what
// it read.

const PICTURE = /\.(gif|png|webp|jpe?g)$/i;
// The skies a theme may bring, by name: a file under `skies/` with another
// name is not drawn.
const SKY_NAMES = ['clear-day', 'clear-hot', 'clear-night', 'partly-day', 'partly-night', 'cloudy', 'fog', 'drizzle', 'rain', 'rain-heavy', 'snow', 'snow-heavy', 'thunder', 'thunder-night'];
const OFF = /^(off|false|no|0)$/i;

// The names under a skies folder that are no sky: not one of the names, or
// not a picture the face reads.
function unknownSkies(names) {
  return (names || []).filter(function (n) {
    const base = String(n).replace(/^.*\//, '');
    if (base[0] === '.') return false;
    return !PICTURE.test(base) || SKY_NAMES.indexOf(base.replace(PICTURE, '')) === -1;
  }).map(function (n) { return String(n).replace(/^.*\//, ''); });
}
// A field's own font: any `<field>.font` key; the display's style fonts are
// `font.<style>` and a size is `<field>.font.size`, neither of which this is.
const OWN_FONT = /^(?!font\.)[a-z][a-z0-9.]*\.font$/;

// Whether a version string a is older than b (1.2.3 against 1.10.0).
function older(a, b) {
  const pa = String(a || '').split('.').map(Number), pb = String(b || '').split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const x = pa[i] || 0, y = pb[i] || 0;
    if (x !== y) return x < y;
  }
  return false;
}

// The glass-evo section: the component and its pairing with this Glass,
// who holds the screen and with which binary, the views, the look and its
// own files, the picture, When the player stops, the face size, the
// forecast. `problems` names what is wrong in codes the page puts in words.
function evo(f) {
  f = f || {};
  const c = f.component || {};
  const installed = c.installed || null;
  const owner = (f.owner && f.owner.owner) || 'kiosk';
  const problems = [];
  const started = f.displayStartedAt ? Date.parse(f.displayStartedAt) : NaN;
  const componentAt = f.componentInstalledAt ? Date.parse(f.componentInstalledAt) : NaN;
  const displayOlder = owner === 'glass-evo' && !isNaN(started) && !isNaN(componentAt) && started < componentAt;
  if (displayOlder) problems.push('display-older-than-component');
  if (installed && c.outdated) problems.push('component-outdated');
  const look = f.look || {};
  const settings = look.settings || {};
  const builtIn = look.builtIn || {};
  const name = String(settings.theme || '').trim();
  const found = name ? (look.looks || []).find(function (l) { return l.name === name; }) : null;
  if (name && !found) problems.push('look-missing');
  const picture = String(settings['idle.picture'] || '').trim();
  const pictureFound = !!(f.picture && f.picture.found);
  if (picture && !pictureFound) problems.push('picture-missing');
  const w = f.weather || {};
  const reading = w.reading || null;
  const readingAge = reading && reading.at ? Math.max(0, Math.round((f.now || Date.now()) / 1000 - reading.at)) : null;
  if (w.place && !reading) problems.push('no-reading');
  return {
    component: installed ? { version: installed.version, available: !!installed.available, requires: installed.requires || null } : null,
    least: c.least || null,
    outdated: !!(installed && c.outdated),
    previous: c.previous && c.previous.version ? c.previous.version : null,
    owner: owner,
    holdMode: (f.owner && f.owner.mode) || null,
    displayBinary: f.displayBinary || null,
    displayStartedAt: f.displayStartedAt || null,
    componentInstalledAt: f.componentInstalledAt || null,
    displayOlder: displayOlder,
    views: f.module ? { mode: f.module.mode || null, face: !!f.module.face, has: !!f.module.has, version: f.module.version || null } : null,
    pages: typeof f.pages === 'number' ? f.pages : null,
    look: {
      name: name || null,
      source: !name ? 'builtin' : !found ? 'missing' : found.shipped ? 'shipped' : 'user',
      skies: Array.isArray(look.files) ? look.files.filter(function (x) { return PICTURE.test(String(x.path || '')); }).length : 0,
      themeBrings: !!look.themeLook,
      themeFolder: look.themeLook && look.themeLook.folder ? look.themeLook.folder : null,
      themeSkies: Array.isArray(f.themeSkies) ? f.themeSkies.filter(function (n) { return PICTURE.test(String(n)); }).length : 0,
      unknownSkies: unknownSkies((Array.isArray(look.files) ? look.files.map(function (x) { return x.path; }) : []).concat(Array.isArray(f.themeSkies) ? f.themeSkies : []))
    },
    picture: picture ? { name: picture, found: pictureFound } : null,
    idleWait: String(settings['idle.wait'] || builtIn['idle.wait'] || 'none'),
    // The plugin's persist period, which "After the persist period" waits for.
    persistS: typeof f.persistS === 'number' ? f.persistS : null,
    faceSize: f.faceSize || null,
    motion: !OFF.test(String(settings['weather.motion'] || builtIn['weather.motion'] || 'on')),
    smallBoard: look.frostSuits === false,
    weather: {
      on: !!w.place,
      place: w.place && w.place.name ? w.place.name : null,
      unit: w.unit || 'C',
      span: String(settings['weather.span'] || builtIn['weather.span'] || 'today'),
      readingAgeS: readingAge,
      error: w.error || ''
    },
    problems: problems
  };
}

// The remotes connected, each with what it is, its version and whether it
// is behind the latest release of its product; `standing` is
// remotebehind.standing, handed in so this stays pure.
function remoteRows(remotes, latest, standing) {
  return (Array.isArray(remotes) ? remotes : []).map(function (r) {
    const s = standing ? standing(r, latest) : { product: null, version: null, latest: null, behind: null };
    return { name: r.name || r.id || '?', product: s.product || null, version: s.version || null, latest: s.latest || null, behind: s.behind };
  });
}

// The keys of a theme's text that name a picture, as the display's own
// reader (lead's tailor) has them: a `*` is one segment, a `**` one or more.
const PICTURE_KEYS = ['bgr.filename', 'fgr.filename', 'indicator.filename', 'bar.filename', 'reflection.filename', 'albumart.mask', 'vinyl.filename', 'reel.*.filename', 'tonearm.filename', 'button.*.image', '*.head.image', '*.knob.image', '*.slider.tip', '*.slider.track', '*.icon', '*.marker.*.image', 'screen.bgr'];
// A record's, a reel's or the tonearm's picture may be a list whose earlier
// names are looked for in the track's own folder; only the last is the theme's.
const LAST_OF_LIST = ['vinyl.filename', 'tonearm.filename', 'reel.*.filename'];

function keyMatches(pattern, key) {
  const go = function (p, k) {
    if (!p.length && !k.length) return true;
    if (!p.length || !k.length) return false;
    if (p[0] === '**') { for (let n = 1; n <= k.length; n++) { if (go(p.slice(1), k.slice(n))) return true; } return false; }
    if (p[0] === '*' || p[0] === k[0]) return go(p.slice(1), k.slice(1));
    return false;
  };
  return go(pattern.split('.'), key.split('.'));
}

// Whether a file the theme names may be absent without harm: the display
// has a fallback it documents. A field's own font falls back to the
// style's font; a knob whose picture is not there is drawn as the arc.
function fileIsOptional(key) {
  const k = String(key).trim().toLowerCase();
  return OWN_FONT.test(k) || keyMatches('*.knob.image', k);
}

// Every file a theme's text names, each with the key that names it,
// resolved as the display resolves it (an absolute path as it is; a
// relative one in the theme folder, a font also under `font.path`) and
// whether it is there: `found`, `missing` (named and not there: the theme
// has a hole), `optional` (missing, and the display does without it),
// `elsewhere` (a path outside the theme that is there).
function themeFiles(metersText, themeDir, fontPath, exists) {
  const out = [];
  const seen = new Map();
  let section = '';
  String(metersText || '').split(/\r?\n/).forEach(function (line) {
    const trimmed = line.trim();
    if (!trimmed || trimmed[0] === '#' || trimmed[0] === ';') return;
    // A file named by the same key in many meters is one entry naming every section.
    if (trimmed[0] === '[') { section = trimmed.replace(/^\[/, '').replace(/\]$/, '').trim(); return; }
    const at = trimmed.indexOf('=');
    if (at === -1) return;
    const key = trimmed.slice(0, at).trim().toLowerCase();
    const value = trimmed.slice(at + 1).trim();
    const font = OWN_FONT.test(key);
    const picture = !font && PICTURE_KEYS.some(function (p) { return keyMatches(p, key); });
    if (!font && !picture) return;
    let files = font ? [value] : value.split(',');
    if (picture && LAST_OF_LIST.some(function (p) { return keyMatches(p, key); })) files = files.slice(-1);
    files.forEach(function (raw) {
      const file = raw.trim();
      if (!file || file.toLowerCase() === 'none') return;
      const id = key + '=' + file;
      if (seen.has(id)) { const have = seen.get(id); if (section && have.sections.indexOf(section) === -1) have.sections.push(section); return; }
      // As the display looks: a font at its absolute path, else inside the
      // theme folder, else under font.path (intake's resolve_own_font); a
      // picture inside the theme folder, or at its absolute path.
      const absolute = file.startsWith('/');
      const candidates = [];
      if (absolute) candidates.push(file);
      if (font) {
        if (themeDir) candidates.push(themeDir.replace(/\/$/, '') + '/' + file.replace(/^\/+/, ''));
        if (fontPath) candidates.push(fontPath.replace(/\/$/, '') + '/' + file.replace(/^\/+/, ''));
      } else if (!absolute && themeDir) {
        candidates.push(themeDir.replace(/\/$/, '') + '/' + file.replace(/^\.\//, ''));
      }
      const found = candidates.find(function (p) { return exists(p); }) || null;
      const state = found ? (found === file ? 'elsewhere' : 'found') : fileIsOptional(key) ? 'optional' : 'missing';
      const item = { key: key, file: file, found: found, state: state, sections: section ? [section] : [] };
      seen.set(id, item);
      out.push(item);
    });
  });
  return out;
}

// The counts a card or a sheet row shows.
function themeFilesSummary(files) {
  const count = function (s) { return files.filter(function (f) { return f.state === s; }).length; };
  return { named: files.length, found: count('found') + count('elsewhere'), missing: count('missing'), optional: count('optional') };
}

// The fonts alone, as the status sheet of 0.9.6 listed them.
function themeFonts(metersText, themeDir, fontPath, exists) {
  return themeFiles(metersText, themeDir, fontPath, exists).filter(function (f) { return OWN_FONT.test(f.key); }).map(function (f) { return { key: f.key, file: f.file, found: f.found }; });
}

module.exports = { evo, remoteRows, themeFonts, themeFiles, themeFilesSummary, fileIsOptional, keyMatches, older, unknownSkies, SKY_NAMES };
