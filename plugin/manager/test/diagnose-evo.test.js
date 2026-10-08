'use strict';

// The checks of glass-evo's own: the face, the look panel and the forecast,
// over the status sheet's glass-evo section and the theme's own fonts.

const test = require('node:test');
const assert = require('node:assert');
const { diagnose } = require('../diagnose');

const NOW = Date.parse('2026-10-08T12:00:00Z');

function evoFacts() {
  return {
    component: { version: '0.2.4', available: true, requires: '0.9.3' }, least: '0.2.4', outdated: false, previous: null,
    owner: 'glass-evo', holdMode: 'kms',
    displayBinary: '/data/INTERNAL/glass/evo/bin/arm/glass-evo', displayStartedAt: '2026-10-08T11:00:00Z', componentInstalledAt: '2026-10-08T10:00:00Z', displayOlder: false,
    views: { mode: 'follow', face: true, has: true, version: '0.2.4' }, pages: 1,
    look: { name: 'Mine', source: 'user', skies: 2, themeBrings: false, themeFolder: null, themeSkies: 0, unknownSkies: [] },
    picture: { name: 'lake.jpg', found: true }, idleWait: 'none', persistS: 15, faceSize: 'normal', motion: true, smallBoard: false,
    weather: { on: true, place: 'Bergen, Vestland, Norway', unit: 'C', span: 'week', readingAgeS: 900, error: '' },
    problems: []
  };
}

function facts(evoOver, over) {
  const evo = evoOver === null ? null : Object.assign(evoFacts(), evoOver || {});
  return Object.assign({
    now: NOW, update: null, log: [], page: { module: '0.2.4' },
    status: Object.assign({ version: '0.9.7', arch: 'arm', binary: true, running: true, channel: { status: 'play' }, activeTheme: '1280x720_theme', themeFonts: [], evo: evo, sheet: { housekeeping: { problems: [] } } }, over || {})
  });
}

const ids = (result, kind) => result.findings.filter((x) => !kind || x.kind === kind).map((x) => x.check);
const one = (result, check) => result.findings.find((x) => x.check === check);

test('the three symptoms of glass-evo find no cause on a player in good order, and check what they check', () => {
  const face = diagnose('face', facts());
  assert.strictEqual(face.found, false, ids(face, 'cause').join(' '));
  assert.deepStrictEqual(ids(face, 'ok'), ['evo-component', 'evo-owner', 'evo-display', 'evo-look', 'evo-picture', 'forecast-place', 'forecast-reading']);
  const look = diagnose('look', facts());
  assert.strictEqual(look.found, false);
  assert.deepStrictEqual(ids(look, 'ok'), ['evo-component', 'evo-display', 'evo-module', 'evo-look', 'evo-picture']);
  const forecast = diagnose('forecast', facts());
  assert.strictEqual(forecast.found, false);
  assert.deepStrictEqual(ids(forecast, 'ok'), ['evo-component', 'evo-owner', 'evo-display', 'forecast-place', 'forecast-reading']);
  // A sheet without the section (a Glass before 0.9.6, or a read that failed) says nothing of glass-evo.
  assert.deepStrictEqual(ids(diagnose('face', facts(null))), []);
});

test('the component: not installed, older than this Glass works with, no binary for this player', () => {
  assert.strictEqual(one(diagnose('face', facts({ component: null })), 'evo-component').key, 'DIAG_EVO_NOT_INSTALLED');
  assert.deepStrictEqual(ids(diagnose('face', facts({ component: null }))), ['evo-component'], 'nothing else is asked of a player without it');
  const old = one(diagnose('look', facts({ outdated: true, least: '0.2.4', component: { version: '0.2.0', available: true } })), 'evo-component');
  assert.strictEqual(old.kind, 'cause');
  assert.deepStrictEqual(old.with, { version: '0.2.0', least: '0.2.4' });
  assert.strictEqual(old.go, 'system');
  const none = one(diagnose('forecast', facts({ component: { version: '0.2.4', available: false } })), 'evo-component');
  assert.strictEqual(none.key, 'DIAG_EVO_NO_BINARY');
  assert.strictEqual(none.with.arch, 'arm');
});

test('the screen: the kiosk owns it, the display is older than the install, or runs Glass\'s own binary', () => {
  const kiosk = diagnose('face', facts({ owner: 'kiosk', displayBinary: null }));
  assert.strictEqual(one(kiosk, 'evo-owner').key, 'DIAG_EVO_KIOSK_OWNS');
  assert.strictEqual(one(kiosk, 'evo-owner').go, 'screen');
  assert.strictEqual(one(kiosk, 'evo-display'), undefined, 'under the kiosk the display is not the face\'s');
  const older = one(diagnose('look', facts({ displayOlder: true })), 'evo-display');
  assert.strictEqual(older.key, 'DIAG_EVO_DISPLAY_OLDER');
  assert.strictEqual(older.kind, 'cause');
  const glass = one(diagnose('forecast', facts({ displayBinary: null })), 'evo-display');
  assert.strictEqual(glass.key, 'DIAG_EVO_GLASS_BINARY');
  assert.strictEqual(one(diagnose('face', facts({ displayStartedAt: null, displayBinary: null })), 'evo-display'), undefined, 'no display, nothing to say');
});

test('the module: none in the component, the views at the theme alone, a stale one in this page', () => {
  const none = one(diagnose('look', facts({ views: { mode: 'face', face: false, has: false, version: null } })), 'evo-module');
  assert.strictEqual(none.key, 'DIAG_EVO_NO_MODULE');
  assert.strictEqual(none.go, 'system');
  const alone = one(diagnose('remotes', facts({ views: { mode: 'theme', face: false, has: true, version: null } })), 'evo-module');
  assert.strictEqual(alone.key, 'DIAG_EVO_VIEWS_THEME_ALONE');
  assert.strictEqual(alone.kind, 'note');
  assert.strictEqual(one(diagnose('look', facts()), 'evo-module').key, 'DIAG_EVO_MODULE');
  const f = facts(); f.page = { module: '0.2.3' };
  const old = one(diagnose('look', f), 'evo-module');
  assert.strictEqual(old.key, 'DIAG_EVO_MODULE_STALE');
  assert.deepStrictEqual(old.with, { page: '0.2.3', installed: '0.2.4' });
  const unknown = facts(); unknown.page = { module: null };
  assert.strictEqual(one(diagnose('look', unknown), 'evo-module').key, 'DIAG_EVO_MODULE', 'a page that drew nothing yet is not stale');
});

test('the look and the picture: a look named but not installed, the theme\'s own look, a picture that is missing', () => {
  const missing = one(diagnose('face', facts({ look: { name: 'Gone', source: 'missing', skies: 0, themeBrings: false, themeFolder: null, themeSkies: 0, unknownSkies: [] } })), 'evo-look');
  assert.strictEqual(missing.key, 'DIAG_EVO_LOOK_MISSING');
  assert.strictEqual(missing.with.name, 'Gone');
  const theme = one(diagnose('look', facts({ look: { name: 'Mine', source: 'user', skies: 0, themeBrings: true, themeFolder: '1280x720_theme', themeSkies: 1, unknownSkies: [] } })), 'evo-look');
  assert.strictEqual(theme.key, 'DIAG_EVO_THEME_LOOK');
  assert.strictEqual(theme.kind, 'note');
  assert.strictEqual(theme.with.folder, '1280x720_theme');
  assert.strictEqual(one(diagnose('look', facts({ look: { name: null, source: 'builtin', skies: 0, themeBrings: false, themeFolder: null, themeSkies: 0, unknownSkies: [] } })), 'evo-look').key, 'DIAG_EVO_LOOK_BUILTIN');
  const picture = one(diagnose('face', facts({ picture: { name: 'lake.jpg', found: false } })), 'evo-picture');
  assert.strictEqual(picture.key, 'DIAG_EVO_PICTURE_MISSING');
  assert.strictEqual(picture.kind, 'cause');
  assert.strictEqual(one(diagnose('face', facts({ picture: null })), 'evo-picture'), undefined, 'no picture named, nothing to check');
  const skies = one(diagnose('forecast', facts({ look: Object.assign(evoFacts().look, { unknownSkies: ['rainy.gif', 'notes.txt'] }) })), 'evo-skies');
  assert.strictEqual(skies.kind, 'note');
  assert.strictEqual(skies.with.names, 'rainy.gif, notes.txt');
});

test('the forecast: no place, no reading, a stale reading, a request that failed', () => {
  const off = diagnose('forecast', facts({ weather: { on: false, place: null, unit: 'C', span: 'today', readingAgeS: null, error: '' } }));
  assert.strictEqual(one(off, 'forecast-place').key, 'DIAG_FORECAST_NO_PLACE');
  assert.strictEqual(one(off, 'forecast-reading'), undefined, 'no place, no reading to ask about');
  const none = one(diagnose('face', facts({ weather: { on: true, place: 'X', unit: 'C', span: 'today', readingAgeS: null, error: 'timed out' } })), 'forecast-reading');
  assert.strictEqual(none.key, 'DIAG_FORECAST_NO_READING');
  assert.strictEqual(none.with.error, 'timed out');
  const stale = one(diagnose('forecast', facts({ weather: { on: true, place: 'X', unit: 'C', span: 'today', readingAgeS: 4 * 3600, error: '' } })), 'forecast-reading');
  assert.strictEqual(stale.key, 'DIAG_FORECAST_STALE');
  assert.deepStrictEqual(stale.with, { minutes: 240, error: '-' });
  const failed = one(diagnose('forecast', facts({ weather: { on: true, place: 'X', unit: 'C', span: 'today', readingAgeS: 1800, error: 'HTTP 503' } })), 'forecast-reading');
  assert.strictEqual(failed.key, 'DIAG_FORECAST_ERROR');
  assert.strictEqual(failed.kind, 'note');
  assert.strictEqual(one(diagnose('forecast', facts()), 'forecast-reading').with.minutes, 15);
  // The place itself is never in a finding: it is personal, as an address is.
  diagnose('forecast', facts()).findings.forEach((x) => assert.ok(JSON.stringify(x).indexOf('Bergen') === -1, x.key + ' carries no place'));
});

test('the wait after a stop, the skies moving on a small board, a theme\'s own font that is missing', () => {
  const wait = one(diagnose('face', facts({ idleWait: 'persist', persistS: 15 })), 'idle-wait');
  assert.strictEqual(wait.key, 'DIAG_IDLE_WAIT_PERSIST');
  assert.strictEqual(wait.with.seconds, 15);
  assert.strictEqual(one(diagnose('face', facts({ idleWait: 'persist', persistS: 0 })), 'idle-wait').key, 'DIAG_IDLE_WAIT_NO_PERSIST');
  assert.strictEqual(one(diagnose('face', facts({ idleWait: 'none', persistS: 0 })), 'idle-wait'), undefined);
  const small = one(diagnose('slow', facts({ motion: true, smallBoard: true })), 'skies-motion');
  assert.strictEqual(small.key, 'DIAG_SKIES_MOTION_SMALL');
  assert.strictEqual(small.kind, 'note');
  assert.strictEqual(one(diagnose('slow', facts({ motion: false, smallBoard: true })), 'skies-motion'), undefined, 'motion off costs nothing');
  assert.strictEqual(one(diagnose('slow', facts({ motion: true, smallBoard: false })), 'skies-motion'), undefined);
  const fonts = [{ key: 'time.total.font', file: 'fonts/Another.ttf', found: null }, { key: 'playinfo.title.font', file: 'fonts/Mine.ttf', found: '/t/fonts/Mine.ttf' }];
  const font = one(diagnose('other', facts({}, { themeFonts: fonts })), 'theme-fonts');
  assert.strictEqual(font.key, 'DIAG_THEME_FONT_MISSING');
  assert.deepStrictEqual(font.with, { theme: '1280x720_theme', files: 'time.total.font = fonts/Another.ttf' });
  assert.strictEqual(one(diagnose('screen', facts({}, { themeFonts: [fonts[1]] })), 'theme-fonts'), undefined, 'every font found, nothing to say');
  assert.strictEqual(one(diagnose('screen', facts({}, { themeFonts: null })), 'theme-fonts'), undefined, 'no theme read, nothing to say');
});
