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

// A theme's own font files: every `<field>.font` its meters.txt names,
// resolved as the display resolves them (an absolute path, the theme
// folder, then `font.path`), and whether the file is there.
function themeFonts(metersText, themeDir, fontPath, exists) {
  const out = [];
  const seen = new Set();
  String(metersText || '').split(/\r?\n/).forEach(function (line) {
    const m = /^\s*([a-z.]+\.font)\s*=\s*(.+?)\s*$/.exec(line);
    if (!m || !OWN_FONT.test(m[1])) return;
    const value = m[2].replace(/^\/+/, '');
    const key = m[1] + '=' + value;
    if (seen.has(key)) return;
    seen.add(key);
    const candidates = [];
    if (m[2].startsWith('/')) candidates.push(m[2]);
    if (themeDir) candidates.push(themeDir.replace(/\/$/, '') + '/' + value);
    if (fontPath) candidates.push(fontPath.replace(/\/$/, '') + '/' + value);
    const found = candidates.find(function (p) { return exists(p); }) || null;
    out.push({ key: m[1], file: m[2], found: found });
  });
  return out;
}

module.exports = { evo, remoteRows, themeFonts, older, unknownSkies, SKY_NAMES };
