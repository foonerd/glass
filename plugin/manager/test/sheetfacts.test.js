'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { evo, remoteRows, themeFonts, themeFiles, themeFilesSummary, fileIsOptional, keyMatches, older, unknownSkies } = require('../sheetfacts');

const NOW = Date.parse('2026-10-08T12:00:00Z');

function facts(over) {
  return Object.assign({
    component: { installed: { version: '0.2.4', available: true, requires: '0.9.3' }, least: '0.2.4', outdated: false, previous: { version: '0.2.3' } },
    owner: { owner: 'glass-evo', mode: 'own-x' },
    displayBinary: '/data/INTERNAL/glass/evo/bin/glass-evo',
    displayStartedAt: '2026-10-08T11:00:00Z',
    componentInstalledAt: '2026-10-08T10:00:00Z',
    module: { mode: 'follow', face: true, has: true, version: '0.2.4' },
    pages: 2,
    look: {
      settings: { theme: 'Mine', 'idle.picture': 'lake.jpg', 'idle.wait': 'persist', 'weather.span': 'week' },
      builtIn: { 'idle.wait': 'none', 'weather.span': 'today' },
      looks: [{ name: 'Mine', shipped: false }, { name: 'Example', shipped: true }],
      files: [{ path: 'skies/rain.gif' }, { path: 'skies/clear-day.png' }],
      themeLook: { folder: '1280x400_Theme' },
      frostSuits: true
    },
    themeSkies: ['snow.gif'],
    picture: { found: true },
    persistS: 15,
    faceSize: 'large',
    weather: { place: { name: 'Bergen, Vestland, Norway', latitude: 60.39, longitude: 5.32 }, unit: 'C', reading: { at: NOW / 1000 - 900 }, error: '' },
    now: NOW
  }, over || {});
}

test('the glass-evo section carries the component, the owner, the views, the look, the picture, the forecast', () => {
  const s = evo(facts());
  assert.deepStrictEqual(s.component, { version: '0.2.4', available: true, requires: '0.9.3' });
  assert.strictEqual(s.least, '0.2.4');
  assert.strictEqual(s.previous, '0.2.3');
  assert.strictEqual(s.owner, 'glass-evo');
  assert.strictEqual(s.holdMode, 'own-x');
  assert.strictEqual(s.displayOlder, false, 'started after the install');
  assert.deepStrictEqual(s.views, { mode: 'follow', face: true, has: true, version: '0.2.4' });
  assert.strictEqual(s.pages, 2);
  assert.deepStrictEqual(s.look, { name: 'Mine', source: 'user', skies: 2, themeBrings: true, themeFolder: '1280x400_Theme', themeSkies: 1, unknownSkies: [] });
  assert.deepStrictEqual(s.picture, { name: 'lake.jpg', found: true });
  assert.strictEqual(s.idleWait, 'persist');
  assert.strictEqual(s.persistS, 15);
  assert.strictEqual(s.faceSize, 'large');
  assert.strictEqual(s.motion, true, 'the skies move unless the setting says off');
  assert.strictEqual(s.smallBoard, false);
  assert.deepStrictEqual(s.weather, { on: true, place: 'Bergen, Vestland, Norway', unit: 'C', span: 'week', readingAgeS: 900, error: '' });
  assert.deepStrictEqual(s.problems, []);
});

test('a display started before the component was installed is a problem, under glass-evo alone', () => {
  const s = evo(facts({ displayStartedAt: '2026-10-08T09:00:00Z' }));
  assert.strictEqual(s.displayOlder, true);
  assert.deepStrictEqual(s.problems, ['display-older-than-component']);
  const kiosk = evo(facts({ displayStartedAt: '2026-10-08T09:00:00Z', owner: { owner: 'kiosk', mode: null } }));
  assert.strictEqual(kiosk.displayOlder, false, 'the kiosk\'s display is not the component\'s');
  assert.deepStrictEqual(evo(facts({ displayStartedAt: null })).problems, [], 'no display, nothing to say');
});

test('a component older than this Glass works with, a look or a picture named but missing, a place with no reading', () => {
  assert.deepStrictEqual(evo(facts({ component: { installed: { version: '0.2.0' }, least: '0.2.4', outdated: true } })).problems, ['component-outdated']);
  const gone = evo(facts({ look: Object.assign(facts().look, { settings: { theme: 'Gone' } }) }));
  assert.strictEqual(gone.look.source, 'missing');
  assert.deepStrictEqual(gone.problems, ['look-missing']);
  const noPicture = evo(facts({ picture: { found: false } }));
  assert.deepStrictEqual(noPicture.picture, { name: 'lake.jpg', found: false });
  assert.deepStrictEqual(noPicture.problems, ['picture-missing']);
  const noReading = evo(facts({ weather: { place: { name: 'X' }, unit: 'F', reading: null, error: 'timed out' } }));
  assert.deepStrictEqual(noReading.weather, { on: true, place: 'X', unit: 'F', span: 'week', readingAgeS: null, error: 'timed out' });
  assert.deepStrictEqual(noReading.problems, ['no-reading']);
});

test('the built-in look, a shipped look, no picture, the forecast off, and nothing installed', () => {
  const plain = evo(facts({ look: { settings: { 'weather.motion': 'off' }, builtIn: {}, looks: [], files: [], themeLook: null, frostSuits: false }, themeSkies: [], picture: null, persistS: undefined, weather: { place: null, unit: 'C', reading: null, error: '' } }));
  assert.deepStrictEqual(plain.look, { name: null, source: 'builtin', skies: 0, themeBrings: false, themeFolder: null, themeSkies: 0, unknownSkies: [] });
  assert.strictEqual(plain.picture, null);
  assert.strictEqual(plain.idleWait, 'none');
  assert.strictEqual(plain.persistS, null);
  assert.strictEqual(plain.motion, false);
  assert.strictEqual(plain.smallBoard, true);
  assert.deepStrictEqual(plain.weather, { on: false, place: null, unit: 'C', span: 'today', readingAgeS: null, error: '' });
  const shipped = evo(facts({ look: Object.assign(facts().look, { settings: { theme: 'Example' }, files: [] }) }));
  assert.strictEqual(shipped.look.source, 'shipped');
  assert.strictEqual(shipped.look.skies, 0);
  const none = evo({});
  assert.strictEqual(none.component, null);
  assert.strictEqual(none.owner, 'kiosk');
  assert.strictEqual(none.views, null);
  assert.deepStrictEqual(none.problems, []);
});

test('the remotes each with their product, version and standing, through the standing handed in', () => {
  const standing = function (r, latest) { return { product: r.face ? 'glass-evo' : 'Glass', version: r.release, latest: latest.Glass, behind: r.release !== latest.Glass }; };
  const rows = remoteRows([{ name: 'kitchen', release: '0.9.5' }, { id: 'abc', release: '0.9.4', face: 'glass-evo 0.2.4' }], { Glass: '0.9.5' }, standing);
  assert.deepStrictEqual(rows, [
    { name: 'kitchen', product: 'Glass', version: '0.9.5', latest: '0.9.5', behind: false },
    { name: 'abc', product: 'glass-evo', version: '0.9.4', latest: '0.9.5', behind: true }
  ]);
  assert.deepStrictEqual(remoteRows(null, {}, standing), []);
});

test('a theme\'s own fonts are resolved as the display resolves them, each named once', () => {
  const here = new Set(['/templates/T/fonts/Mine.ttf', '/usr/share/fonts/Other.ttf', '/abs/Abs.otf']);
  const exists = function (p) { return here.has(p); };
  const text = [
    'meter.folder = T',
    'time.font = fonts/Mine.ttf',
    'playinfo.title.font = /Other.ttf',
    'playinfo.artist.font = fonts/Mine.ttf',
    'volume.value.font = /abs/Abs.otf',
    'time.font = fonts/Mine.ttf',
    'playinfo.album.font = fonts/Gone.ttf',
    'font.light = Light.ttf'
  ].join('\n');
  assert.deepStrictEqual(themeFonts(text, '/templates/T/', '/usr/share/fonts', exists), [
    { key: 'time.font', file: 'fonts/Mine.ttf', found: '/templates/T/fonts/Mine.ttf' },
    { key: 'playinfo.title.font', file: '/Other.ttf', found: '/usr/share/fonts/Other.ttf' },
    { key: 'playinfo.artist.font', file: 'fonts/Mine.ttf', found: '/templates/T/fonts/Mine.ttf' },
    { key: 'volume.value.font', file: '/abs/Abs.otf', found: '/abs/Abs.otf' },
    { key: 'playinfo.album.font', file: 'fonts/Gone.ttf', found: null }
  ]);
  assert.deepStrictEqual(themeFonts('', '/t', '', exists), []);
  assert.deepStrictEqual(themeFonts('time.font = x.ttf', '', '', exists), [{ key: 'time.font', file: 'x.ttf', found: null }], 'no folder, nowhere to look');
});

test('a file under skies that is no sky is named, from the look and from the theme on show', () => {
  assert.deepStrictEqual(unknownSkies(['skies/rain.gif', 'skies/Rainy.png', 'skies/clear-day.webp', 'skies/clear-day.svg', 'snow.jpeg', 'storm.gif', '.DS_Store']), ['Rainy.png', 'clear-day.svg', 'storm.gif']);
  assert.deepStrictEqual(unknownSkies([]), []);
  const s = evo(facts({ look: Object.assign(facts().look, { files: [{ path: 'skies/rainy.gif' }] }), themeSkies: ['snow.gif', 'notes.txt'] }));
  assert.deepStrictEqual(s.look.unknownSkies, ['rainy.gif', 'notes.txt']);
});

test('every file a theme names is found, missing, optional or elsewhere, as the display reads it', () => {
  const here = new Set(['/t/gold/gold-bgr.png', '/t/gold/play.png', '/t/gold/fonts/Mine.ttf', '/t/gold/vinyl_disc.png', '/usr/share/fonts/Other.ttf', '/abs/Abs.otf']);
  const exists = function (p) { return here.has(p); };
  const text = [
    '[gold]', 'meter.type = circular', '# a comment',
    'bgr.filename = gold-bgr.png', 'fgr.filename = gold-fgr.png',
    'button.play.image = play.png, play-lit.png',
    'vinyl.filename = disc.png,vinyl_disc.png',
    'volume.knob.image = knob.png',
    'time.total.font = fonts/Mine.ttf', 'playinfo.title.font = /Other.ttf', 'playinfo.album.font = fonts/Gone.ttf', 'volume.value.font = /abs/Abs.otf',
    'font.light = Light.ttf', 'albumart.mask = none', 'reel.left.filename = tape.png,reel-left.png'
  ].join('\n');
  const files = themeFiles(text, '/t/gold/', '/usr/share/fonts', exists);
  assert.deepStrictEqual(files.map(function (f) { return [f.key, f.file, f.state]; }), [
    ['bgr.filename', 'gold-bgr.png', 'found'],
    ['fgr.filename', 'gold-fgr.png', 'missing'],
    ['button.play.image', 'play.png', 'found'],
    ['button.play.image', 'play-lit.png', 'missing'],
    ['vinyl.filename', 'vinyl_disc.png', 'found'],
    ['volume.knob.image', 'knob.png', 'optional'],
    ['time.total.font', 'fonts/Mine.ttf', 'found'],
    // The old form with a leading slash: found under font.path, as the display finds it.
    ['playinfo.title.font', '/Other.ttf', 'found'],
    ['playinfo.album.font', 'fonts/Gone.ttf', 'optional'],
    ['volume.value.font', '/abs/Abs.otf', 'elsewhere'],
    ['reel.left.filename', 'reel-left.png', 'missing']
  ]);
  assert.strictEqual(files[6].found, '/t/gold/fonts/Mine.ttf');
  assert.strictEqual(files[7].found, '/usr/share/fonts/Other.ttf');
  assert.deepStrictEqual(themeFilesSummary(files), { named: 11, found: 6, missing: 3, optional: 2 });
  assert.ok(fileIsOptional('time.elapsed.font') && fileIsOptional('progress.knob.image') && !fileIsOptional('bgr.filename') && !fileIsOptional('font.light'));
  assert.ok(keyMatches('*.marker.*.image', 'volume.marker.3.image') && keyMatches('reel.*.filename', 'reel.right.filename') && !keyMatches('*.icon', 'a.b.icon'));
  assert.deepStrictEqual(themeFiles('', '/t', '', exists), []);
});

test('versions compare by their three numbers', () => {
  assert.strictEqual(older('0.9.5', '0.10.0'), true);
  assert.strictEqual(older('0.10.0', '0.9.5'), false);
  assert.strictEqual(older('0.9.5', '0.9.5'), false);
  assert.strictEqual(older('', '0.1.0'), true);
});
