'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const stable = require('../stable');
const { compareVersions } = require('../update');

const PLUGIN = path.join(__dirname, '..', '..');

const entry = (value, type) => ({ value: value, type: type || typeof value });

// A configuration file's keys by section, as the plugin's reader names
// them: [sdl.env] is the section sdl. Enough to list what a template holds.
function sectionsOf(text) {
  const found = {};
  let section = null;
  text.split('\n').forEach((raw) => {
    const line = raw.trim();
    if (!line || line[0] === '#' || line[0] === ';') return;
    const head = /^\[([^\]]+)\]$/.exec(line);
    if (head) { section = head[1].split('.')[0]; found[section] = found[section] || []; return; }
    const at = line.indexOf('=');
    if (at > 0 && section) found[section].push(line.slice(0, at).trim());
  });
  return found;
}

const DEFAULTS = {
  config: { timeout: entry('60', 'number'), activeFolder: entry('800x480', 'string'), displayOutput: entry('0', 'number'), remotesEnabled: entry(true), fanartEnabled: entry(false), fanart_personal_key: entry('', 'string'), doNotDeleteThemes: entry(false) },
  meter: { current: { meter: 'random', 'meter.folder': '800x480', 'frame.rate': '30', 'base.folder': '/data/INTERNAL/glass/templates', 'exit.on.touch': 'True', 'remote.server.port': '5580', 'debug.level': 'trace' }, sdl: { env: { 'video.display': ':0' } } },
  spectrum: { current: { spectrum: 's.1', 'spectrum.folder': '800x480', 'frame.rate': '30' }, sdl: { env: { 'video.driver': 'dummy' } } }
};

const CURRENT = {
  config: { timeout: entry('5', 'number'), activeFolder: entry('1280x400_Deck', 'string'), displayOutput: entry('1', 'number'), remotesEnabled: entry(false), fanartEnabled: entry(true), fanart_personal_key: entry('abc123', 'string'), doNotDeleteThemes: entry(true),
    testReleases: entry(true), perfProfile: entry('eco', 'string'), viewsFace: entry('theme', 'string'), fromALaterRelease: entry('x', 'string') },
  meter: { current: { meter: 'deck-a', 'meter.folder': '1280x400_Deck', 'frame.rate': '60', 'frame.rate.governor': 'on', 'base.folder': '/somewhere/else', 'exit.on.touch': 'False', 'remote.server.port': '6000', 'debug.level': 'off',
      'screen.rotation': '90', 'screen.driver': 'kmsdrm', 'touch.matrix': '0 1 0 -1 0 1', 'touch.interactive': 'on', 'position.x': '-12', 'face.theme': 'Warm', 'face.size': 'car', 'face.clock.format': '%H:%M:%S', 'face.frost.suits': 'true', 'later.key': '1' },
    sdl: { env: { 'video.display': ':1' } }, strange: { a: '1' } },
  spectrum: { current: { spectrum: 'bars', 'spectrum.folder': '1280x400_Deck', 'frame.rate': '60' }, sdl: { env: { 'video.driver': 'x11' } } }
};

const NOTHING = { screen: false, network: false, owner: false, show: false, look: false, behaviour: false, tuning: false, tests: false };

test('the questions are the parts that apply, each with its suggested answer', () => {
  const plain = stable.questions({});
  assert.deepStrictEqual(plain.map((q) => q.id), ['screen', 'network', 'show', 'behaviour', 'tuning']);
  assert.deepStrictEqual(plain.filter((q) => q.keep).map((q) => q.id), ['screen', 'network'], 'the hardware and the network are kept unless the user says no');
  const all = stable.questions({ holds: true, evo: true, tests: true });
  assert.deepStrictEqual(all.map((q) => q.id), stable.PARTS.map((p) => p.id));
  assert.deepStrictEqual(stable.questions({ evo: true }).map((q) => q.id), ['screen', 'network', 'show', 'look', 'behaviour', 'tuning']);
});

test('an answer is the one given, the suggested one where none was, and no for a part not asked', () => {
  const facts = { evo: true };
  assert.deepStrictEqual(stable.answers(null, facts), { screen: true, network: true, owner: false, show: false, look: false, behaviour: false, tuning: false, tests: false });
  const said = stable.answers({ screen: false, show: true, look: 'yes', owner: true, tests: true }, facts);
  assert.strictEqual(said.screen, false);
  assert.strictEqual(said.show, true);
  assert.strictEqual(said.look, false, 'a word that is no yes or no is the suggested answer');
  assert.strictEqual(said.owner, false, 'glass-evo does not hold the screen: nothing to keep');
  assert.strictEqual(said.tests, false, 'the player takes no test releases: nothing to keep');
});

test('with nothing kept the settings are the defaults, but for what is always kept', () => {
  const planned = stable.plan(CURRENT, DEFAULTS, NOTHING);
  const want = JSON.parse(JSON.stringify(DEFAULTS));
  want.config.fanart_personal_key = entry('abc123', 'string');
  want.config.doNotDeleteThemes = entry(true);
  assert.deepStrictEqual(planned, want);
  assert.strictEqual(planned.config.testReleases, undefined, 'test releases are off again');
  assert.strictEqual(planned.config.fromALaterRelease, undefined, 'a key no table names goes');
  assert.strictEqual(planned.meter.strange, undefined, 'and so does a section none names');
  assert.deepStrictEqual(Object.keys(planned.meter), ['current', 'sdl'], '[current] first');
});

test('a part kept is carried over the defaults, and only that part', () => {
  const screen = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { screen: true }));
  assert.strictEqual(screen.meter.current['screen.rotation'], '90');
  assert.strictEqual(screen.meter.current['screen.driver'], 'kmsdrm');
  assert.strictEqual(screen.meter.current['touch.matrix'], '0 1 0 -1 0 1');
  assert.strictEqual(screen.meter.current['position.x'], '-12');
  assert.deepStrictEqual(screen.meter.sdl, { env: { 'video.display': ':1' } }, 'the screen\'s environment goes with the screen');
  assert.deepStrictEqual(screen.spectrum.sdl, { env: { 'video.driver': 'x11' } });
  assert.deepStrictEqual(screen.config.displayOutput, entry('1', 'number'));
  assert.strictEqual(screen.meter.current['touch.interactive'], undefined, 'the controls under the fingers are a preference, not the panel');
  assert.strictEqual(screen.meter.current['meter.folder'], '800x480');
  assert.strictEqual(screen.meter.current['frame.rate'], '30');

  const show = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { show: true }));
  assert.strictEqual(show.meter.current.meter, 'deck-a');
  assert.strictEqual(show.meter.current['meter.folder'], '1280x400_Deck');
  assert.strictEqual(show.spectrum.current['spectrum.folder'], '1280x400_Deck');
  assert.strictEqual(show.spectrum.current['frame.rate'], '30', 'the spectrum file\'s other keys are the release\'s');
  assert.deepStrictEqual(show.config.activeFolder, entry('1280x400_Deck', 'string'));
  assert.deepStrictEqual(show.config.fanartEnabled, entry(true));
  assert.strictEqual(show.meter.current['screen.rotation'], undefined);

  const look = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { look: true }));
  assert.strictEqual(look.meter.current['face.theme'], 'Warm');
  assert.strictEqual(look.meter.current['face.size'], 'car');
  assert.strictEqual(look.meter.current['face.clock.format'], '%H:%M:%S');
  assert.strictEqual(look.meter.current['face.frost.suits'], undefined, 'the board\'s word is the plugin\'s to note again');
  assert.deepStrictEqual(look.config.viewsFace, entry('theme', 'string'));

  const tuning = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { tuning: true }));
  assert.strictEqual(tuning.meter.current['frame.rate'], '60');
  assert.strictEqual(tuning.meter.current['frame.rate.governor'], 'on');
  assert.strictEqual(tuning.meter.current['debug.level'], 'off');
  assert.deepStrictEqual(tuning.config.perfProfile, entry('eco', 'string'));

  const network = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { network: true }));
  assert.strictEqual(network.meter.current['remote.server.port'], '6000');
  assert.deepStrictEqual(network.config.remotesEnabled, entry(false));

  const behaviour = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { behaviour: true }));
  assert.deepStrictEqual(behaviour.config.timeout, entry('5', 'number'));
  assert.strictEqual(behaviour.meter.current['exit.on.touch'], 'False');
  assert.strictEqual(behaviour.meter.current['touch.interactive'], 'on');

  const tests = stable.plan(CURRENT, DEFAULTS, Object.assign({}, NOTHING, { tests: true }));
  assert.deepStrictEqual(tests.config.testReleases, entry(true));

  // Where the themes are kept is the release's own, whatever is kept.
  const everything = stable.plan(CURRENT, DEFAULTS, { screen: true, network: true, owner: true, show: true, look: true, behaviour: true, tuning: true, tests: true });
  assert.strictEqual(everything.meter.current['base.folder'], '/data/INTERNAL/glass/templates');
  assert.strictEqual(everything.meter.current['later.key'], undefined);
});

test('the plan is made of copies and leaves what it was given as it was', () => {
  const before = JSON.stringify([CURRENT, DEFAULTS]);
  const planned = stable.plan(CURRENT, DEFAULTS, { screen: true, show: true });
  planned.meter.sdl.env['video.display'] = 'changed';
  planned.config.activeFolder.value = 'changed';
  assert.strictEqual(JSON.stringify([CURRENT, DEFAULTS]), before);
  assert.ok(stable.changed(CURRENT, stable.plan(CURRENT, DEFAULTS, NOTHING)) > 10);
  assert.strictEqual(stable.changed(DEFAULTS, stable.plan(DEFAULTS, DEFAULTS, NOTHING)), 0, 'a player at the defaults has nothing changed');
});

test('each release is stepped to the stable one, forward or back, or left', () => {
  assert.deepStrictEqual(stable.step('0.8.51', '0.8.46', compareVersions), { action: 'back', from: '0.8.51', to: '0.8.46' });
  assert.deepStrictEqual(stable.step('0.8.40', '0.8.46', compareVersions), { action: 'forward', from: '0.8.40', to: '0.8.46' });
  assert.deepStrictEqual(stable.step('0.8.46', '0.8.46', compareVersions), { action: 'none', from: '0.8.46', to: '0.8.46' });
  assert.deepStrictEqual(stable.step(null, '0.1.24', compareVersions), { action: 'none', from: null, to: null }, 'glass-evo is not here: nothing to step');
});

// Every setting the plugin ships a default for, reads or writes has a
// class: a new setting is classed when it is added, or this fails.
test('every setting the plugin knows is classed', () => {
  const source = ['index.js', 'manager/server.js', 'manager/logging.js', 'manager/performance.js', 'manager/facelook.js', 'manager/views.js', 'manager/cardash.js']
    .map((file) => { try { return fs.readFileSync(path.join(PLUGIN, file), 'utf8'); } catch (e) { return ''; } }).join('\n');
  const unclassed = [];
  const check = (table, key, from) => { if (stable.classOf(table, key) === null) unclassed.push(table + ': ' + key + ' (' + from + ')'); };

  Object.keys(JSON.parse(fs.readFileSync(path.join(PLUGIN, 'config.json'), 'utf8'))).forEach((key) => check('config', key, 'config.json'));
  (source.match(/config\.(?:get|set)\('([A-Za-z_]+)'/g) || []).forEach((m) => check('config', /'([^']+)'/.exec(m)[1], 'the plugin'));

  const meter = sectionsOf(fs.readFileSync(path.join(PLUGIN, 'config', 'meter.txt.tmpl'), 'utf8'));
  assert.ok(meter.current.length > 40, 'the template was read');
  meter.current.forEach((key) => check('meter', key, 'meter.txt.tmpl'));
  Object.keys(meter).filter((s) => s !== 'current').forEach((section) => check('section', section, 'meter.txt.tmpl'));
  (source.match(/(?:cur|current)\['([a-z0-9.]+)'\]/g) || []).forEach((m) => check('meter', /'([^']+)'/.exec(m)[1], 'the plugin'));

  const spectrum = sectionsOf(fs.readFileSync(path.join(PLUGIN, 'config', 'spectrum.txt.tmpl'), 'utf8'));
  assert.ok(spectrum.current.length > 5, 'the template was read');
  spectrum.current.forEach((key) => check('spectrum', key, 'spectrum.txt.tmpl'));
  Object.keys(spectrum).filter((s) => s !== 'current').forEach((section) => check('section', section, 'spectrum.txt.tmpl'));

  assert.deepStrictEqual(unclassed, []);
  // And a class is a part, or one of the two that are not asked.
  const classes = stable.PARTS.map((p) => p.id).concat([stable.ALWAYS, stable.DEFAULT]);
  Object.keys(stable.TABLES).forEach((table) => stable.TABLES[table].forEach((row) => assert.ok(classes.indexOf(row[1]) !== -1, table + ': ' + row[0])));
});

// ---- the act --------------------------------------------------------------

// A player as the act sees it: an updater, a component and the plugin,
// each writing what it was asked into `calls`, each failing where told to.
function player(options) {
  const o = Object.assign({ glass: '0.8.51', stableGlass: '0.8.46', evo: '0.1.27', stableEvo: '0.1.24', holds: true, fail: {} }, options || {});
  const calls = [];
  const said = (name, extra) => { calls.push(extra === undefined ? name : name + ' ' + extra); };
  const failing = (name) => { if (o.fail[name]) throw new Error(name + ' failed'); };
  const settings = { current: JSON.parse(JSON.stringify(CURRENT)), written: null, restored: null };
  const state = { evo: o.evo, holds: o.holds };
  const updater = {
    version: o.glass,
    stable: async () => { said('glass.stable'); failing('glass.stable'); return { version: o.stableGlass }; },
    stage: async (job, release) => { said('glass.stage', release.version); failing('glass.stage'); return { name: 'glass-' + release.version + '.zip', file: '/staged', version: release.version }; },
    backupName: (word) => 'before-' + word,
    apply: async (job, staged, opts) => { said('glass.apply', staged.version + ' backup=' + opts.backup); failing('glass.apply'); job.state = 'restarting'; job.target = staged.version; }
  };
  const component = {
    least: '0.1.13',
    installed: () => ({ installed: !!state.evo, version: state.evo }),
    stable: async () => { said('evo.stable'); return { version: o.stableEvo }; },
    installRelease: async (job, release, target) => { said('evo.install', release.version + ' for Glass ' + target.glass + ' least ' + target.least); failing('evo.install'); state.evo = release.version; },
    rollback: async () => { said('evo.rollback'); state.evo = o.evo; }
  };
  const plugin = {
    stableFacts: () => ({ holds: state.holds, evo: !!state.evo, tests: true }),
    stableCurrent: () => settings.current,
    stableDefaults: (texts) => { said('defaults', texts.config ? 'from the zip' : 'from the plugin here'); return DEFAULTS; },
    stableWrite: (planned) => { said('settings.write'); failing('settings.write'); settings.written = planned; },
    backupCreate: (name, opts) => { said('backup', name + (opts.automatic ? ' automatic' : '')); return o.fail.backup ? { error: 'GLASS.BACKUP_DISK_FULL' } : { ok: true, name: name }; },
    backupRestore: (name) => { said('settings.restore', name); settings.restored = name; return { ok: true }; },
    setScreenOwner: async (owner) => { said('screen', owner); if (o.fail.screen) return { error: 'GLASS.MANAGER_OWNER_FAILED', message: 'a unit would not start' }; state.holds = false; return { ok: true, changed: true }; },
    componentChanged: () => { said('evo.changed'); },
    restartBackend: () => { said('backend.restart'); }
  };
  const with_ = { updater, component, plugin, texts: async () => ({ package: '{"glassEvo":{"least":"0.1.20"}}', config: '{}', meter: '', spectrum: '' }), least: (text) => JSON.parse(text).glassEvo.least, logger: { info: () => {}, warn: (m) => { said('warned', m); } } };
  return { calls, settings, state, with_ };
}

test('the act goes in an order that changes nothing before what it needs is here', async () => {
  const p = player();
  const job = {};
  const keep = stable.answers(null, { holds: true, evo: true, tests: true });
  const done = await stable.run(job, keep, p.with_);
  assert.deepStrictEqual(p.calls, [
    'glass.stable', 'evo.stable', 'glass.stage 0.8.46', 'defaults from the zip', 'backup before-stable automatic',
    'evo.install 0.1.24 for Glass 0.8.46 least 0.1.20', 'screen kiosk', 'settings.write', 'glass.apply 0.8.46 backup=before-stable'
  ]);
  assert.deepStrictEqual([done.glass.action, done.evo.action, done.backup, done.kept], ['back', 'back', 'before-stable', ['screen', 'network']]);
  assert.deepStrictEqual([job.state, job.target], ['restarting', '0.8.46']);
  // The settings written are the plan of the answers.
  assert.deepStrictEqual(p.settings.written, stable.plan(CURRENT, DEFAULTS, keep));
  assert.strictEqual(p.settings.written.meter.current['screen.rotation'], '90');
  assert.strictEqual(p.settings.written.config.testReleases, undefined);
});

test('a player on the stable release keeps its versions and its backend starts again on the settings', async () => {
  const p = player({ glass: '0.8.46', evo: '0.1.24', holds: false });
  const job = {};
  const done = await stable.run(job, stable.answers({ show: true }, { evo: true }), p.with_);
  assert.deepStrictEqual(p.calls, ['glass.stable', 'evo.stable', 'defaults from the plugin here', 'backup before-stable automatic', 'settings.write', 'backend.restart']);
  assert.deepStrictEqual([done.glass.action, done.evo.action], ['none', 'none']);
  assert.deepStrictEqual([job.state, job.target], ['restarting', '0.8.46']);
  assert.strictEqual(p.settings.written.meter.current['meter.folder'], '1280x400_Deck', 'the theme on show was kept');

  // No glass-evo here: it is not asked after, and a kept screen owner is not touched.
  const bare = player({ evo: null, holds: false });
  await stable.run({}, stable.answers(null, {}), bare.with_);
  assert.ok(bare.calls.indexOf('evo.stable') === -1 && bare.calls.indexOf('screen kiosk') === -1);
  const held = player();
  await stable.run({}, stable.answers({ owner: true }, { holds: true, evo: true }), held.with_);
  assert.ok(held.calls.indexOf('screen kiosk') === -1, 'glass-evo keeps the screen where the user said so');
});

test('a step that fails leaves the player as it was, or puts it back', async () => {
  const keep = stable.answers(null, { holds: true, evo: true, tests: true });
  // Nothing to stage: nothing was touched.
  let p = player({ fail: { 'glass.stage': true } });
  await assert.rejects(stable.run({}, keep, p.with_), /glass.stage failed/);
  assert.deepStrictEqual(p.calls, ['glass.stable', 'evo.stable', 'glass.stage 0.8.46']);
  // No backup: nothing is changed without the way to undo it.
  p = player({ fail: { backup: true } });
  await assert.rejects(stable.run({}, keep, p.with_), { code: 'backup' });
  assert.ok(p.calls.indexOf('evo.install 0.1.24 for Glass 0.8.46 least 0.1.20') === -1 && p.settings.written === null);
  // glass-evo does not go in: the settings were not touched yet.
  p = player({ fail: { 'evo.install': true } });
  await assert.rejects(stable.run({}, keep, p.with_), /evo.install failed/);
  assert.ok(p.settings.written === null && p.calls.indexOf('evo.rollback') === -1 && p.calls.indexOf('screen kiosk') === -1);
  // The screen does not go back: glass-evo is put back, the settings were not written.
  p = player({ fail: { screen: true } });
  await assert.rejects(stable.run({}, keep, p.with_), { code: 'owner' });
  assert.deepStrictEqual(p.calls.slice(-3), ['screen kiosk', 'evo.rollback', 'evo.changed']);
  assert.ok(p.settings.written === null && p.settings.restored === null && p.state.evo === '0.1.27');
  // The writing breaks half way: what was written is put back from the backup.
  p = player({ fail: { 'settings.write': true } });
  await assert.rejects(stable.run({}, keep, p.with_), /settings.write failed/);
  assert.deepStrictEqual(p.calls.slice(-4), ['settings.write', 'settings.restore before-stable', 'evo.rollback', 'evo.changed']);
  // Glass does not go in: the settings come back from the backup, glass-evo from the one before.
  p = player({ fail: { 'glass.apply': true } });
  await assert.rejects(stable.run({}, keep, p.with_), /glass.apply failed/);
  assert.deepStrictEqual(p.calls.slice(-4), ['glass.apply 0.8.46 backup=before-stable', 'settings.restore before-stable', 'evo.rollback', 'evo.changed']);
  assert.strictEqual(p.state.evo, '0.1.27');
});
