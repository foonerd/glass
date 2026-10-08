'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { buildZip } = require('./zipwriter');
const { Component, leastOf, manifestOf, misfit, installedAt, pairPlan, pair } = require('../component');

const sha = (data) => crypto.createHash('sha256').update(data).digest('hex');

// A component's zip as glass-evo's packaging makes it: the manifest, a
// binary for each architecture, the looks, the built-in look.
function componentZip(version, options) {
  options = options || {};
  const binaries = {};
  const files = [];
  ['arm', 'x64'].forEach(function (arch) {
    const data = Buffer.from('binary ' + version + ' for ' + arch);
    binaries[arch] = { path: 'bin/' + arch + '/glass-evo', sha256: options.wrongBinary ? 'f'.repeat(64) : sha(data) };
    files.push({ name: 'bin/' + arch + '/glass-evo', data: data, mode: 0o100755 });
  });
  const manifest = { name: 'glass-evo', version: options.says || version, binaries: binaries };
  if (options.requires) manifest.requires = { glass: options.requires };
  if (options.module) {
    const module = Buffer.from('the face for a browser, ' + version);
    manifest.face = { path: options.module.path || 'face/glass-evo-face.wasm', sha256: options.module.wrong ? 'e'.repeat(64) : sha(module) };
    if (!options.module.absent) files.push({ name: 'face/glass-evo-face.wasm', data: module });
  }
  files.unshift({ name: 'manifest.json', data: JSON.stringify(manifest) });
  files.push({ name: 'face.txt', data: '[theme]\nname = Example\n' });
  files.push({ name: 'themes/Warm/face.txt', data: '[theme]\nname = Warm\n' });
  files.push({ name: 'themes/Warm/notes.md', data: 'not a look' });
  files.push({ name: '../escape.txt', data: 'no' });
  return buildZip(files);
}

function release(version, zip, digest) {
  return { tag_name: 'v' + version, name: version, body: 'notes', published_at: '2026-10-01T12:00:00Z', html_url: 'https://example/release',
    assets: [{ name: 'glass-evo-' + version + '.zip', size: zip.length, digest: 'sha256:' + (digest || sha(zip)), browser_download_url: 'zip' }] };
}

// A player's folders and a component manager over them, fetching from `shelf`.
function rig(options) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-component-'));
  const shelf = { release: null, zip: null };
  const component = new Component(Object.assign({
    dir: path.join(root, 'evo'),
    stateDir: path.join(root, 'state'),
    glass: '0.8.0',
    least: '0.1.9',
    arch: function () { return 'arm'; },
    logger: { info: function () {}, warn: function () {} },
    releasesUrl: 'releases',
    fetch: async function (url, opts) {
      if (url === 'releases') return { body: Buffer.from(JSON.stringify(shelf.release)) };
      opts.sink(shelf.zip.subarray(0, 7), 7);
      opts.sink(shelf.zip.subarray(7), shelf.zip.length);
      return {};
    }
  }, options || {}));
  const offer = function (version, zipOptions, digest) {
    shelf.zip = componentZip(version, zipOptions);
    shelf.release = release(version, shelf.zip, digest);
  };
  return { root, component, offer, job: function () { return { state: 'queued', progress: null }; } };
}

test('a Glass names the least glass-evo in its package, a manifest the least Glass', () => {
  assert.equal(leastOf(JSON.stringify({ name: 'glass', glassEvo: { least: '0.1.9' } })), '0.1.9');
  assert.equal(leastOf(JSON.stringify({ name: 'glass' })), null);
  assert.equal(leastOf('not json'), null);
  assert.equal(leastOf(JSON.stringify({ glassEvo: { least: 'soon' } })), null);
  const m = manifestOf(JSON.stringify({ name: 'glass-evo', version: '0.1.9', binaries: {}, requires: { glass: '0.8.0' } }));
  assert.deepEqual([m.version, m.requires], ['0.1.9', '0.8.0']);
  assert.equal(manifestOf(JSON.stringify({ name: 'glass-evo', version: '0.1.8', binaries: {} })).requires, null, 'an early one names none');
  assert.throws(() => manifestOf('{'), { code: 'bad-manifest' });
  assert.throws(() => manifestOf(JSON.stringify({ name: 'other', version: '1.0.0', binaries: {} })), { code: 'bad-manifest' });
  assert.throws(() => manifestOf(JSON.stringify({ name: 'glass-evo', version: 'next', binaries: {} })), { code: 'bad-manifest' });
});

test('a component goes with a Glass only inside both ranges and with a binary for the player', () => {
  const bin = { path: 'bin/arm/glass-evo', sha256: 'a'.repeat(64) };
  const m = { version: '0.2.0', binaries: { arm: bin }, requires: '0.8.0' };
  assert.equal(misfit(m, '0.8.0', 'arm', '0.1.9'), null);
  assert.equal(misfit(m, '0.9.3', 'arm', '0.2.0'), null);
  assert.deepEqual(misfit(m, '0.7.97', 'arm', '0.1.9'), { error: 'needs-glass', needs: '0.8.0' });
  assert.deepEqual(misfit(m, '0.8.0', 'arm', '0.2.1'), { error: 'too-old', least: '0.2.1' });
  assert.deepEqual(misfit(m, '0.8.0', 'armv8', '0.1.9'), { error: 'no-binary', arch: 'armv8' });
  assert.deepEqual(misfit({ version: '0.2.0', binaries: { arm: { path: '../x', sha256: 'a'.repeat(64) } }, requires: null }, '0.8.0', 'arm', null), { error: 'no-binary', arch: 'arm' });
  assert.equal(misfit({ version: '0.1.8', binaries: { arm: bin }, requires: null }, '0.7.0', 'arm', null), null, 'no ranges named, none held to');
});

test('a Glass about to be installed asks for a newer component, or cannot be', () => {
  const here = { installed: true, version: '0.1.9', requires: '0.8.0' };
  assert.deepEqual(pairPlan(null, { version: '0.9.0', least: '0.3.0' }), { action: 'none' }, 'no component, nothing to pair');
  assert.deepEqual(pairPlan({ installed: false }, { version: '0.9.0', least: '0.3.0' }), { action: 'none' });
  assert.deepEqual(pairPlan(here, { version: '0.8.1', least: '0.1.9' }), { action: 'none' });
  assert.deepEqual(pairPlan(here, { version: '0.8.1', least: null }), { action: 'none' });
  assert.deepEqual(pairPlan(here, { version: '0.9.0', least: '0.2.0' }), { action: 'update', least: '0.2.0' });
  assert.deepEqual(pairPlan(here, { version: '0.7.97', least: null }), { action: 'refuse', needs: '0.8.0' }, 'a step back of Glass below what the component needs');
});

test('getting the component: checked, unpacked for this player, and kept through an update and a way back', async () => {
  const { root, component, offer, job } = rig();
  await component.init();
  assert.deepEqual(installedAt(path.join(root, 'evo'), 'arm'), { installed: false, available: false, version: null, binary: null, arch: 'arm', requires: null, face: null });
  offer('0.1.9', { requires: '0.8.0' });
  let view = await component.check(true);
  assert.deepEqual([view.installed, view.available, view.outdated, view.behind, view.latest.version], [null, true, false, false, '0.1.9']);

  const j = job();
  assert.deepEqual(await component.install(j), { from: null, to: '0.1.9' });
  assert.equal(j.state, 'applying');
  const evo = path.join(root, 'evo');
  assert.equal(fs.readFileSync(path.join(evo, 'bin/arm/glass-evo'), 'utf8'), 'binary 0.1.9 for arm');
  assert.equal(fs.statSync(path.join(evo, 'bin/arm/glass-evo')).mode & 0o777, 0o755);
  assert.ok(!fs.existsSync(path.join(evo, 'bin/x64')), 'another architecture\'s binary is left in the zip');
  assert.ok(fs.existsSync(path.join(evo, 'face.txt')) && fs.existsSync(path.join(evo, 'themes/Warm/face.txt')));
  assert.ok(!fs.existsSync(path.join(evo, 'themes/Warm/notes.md')) && !fs.existsSync(path.join(root, 'escape.txt')), 'nothing but what a face reads');
  assert.deepEqual(fs.readdirSync(path.join(root, 'state')), ['latest.json'], 'the download is gone');
  view = component.view();
  assert.deepEqual([view.installed, view.available, view.previous], [{ version: '0.1.9', available: true, requires: '0.8.0' }, false, null]);
  await assert.rejects(component.install(job()), { code: 'up-to-date' });

  offer('0.2.0', { requires: '0.8.0' });
  view = await component.check(true);
  assert.equal(view.available, true);
  assert.deepEqual(await component.install(job()), { from: '0.1.9', to: '0.2.0' });
  assert.deepEqual([component.view().installed.version, component.view().previous], ['0.2.0', { version: '0.1.9', requires: '0.8.0' }]);
  assert.deepEqual(await component.rollback(), { from: '0.2.0', to: '0.1.9' });
  assert.deepEqual([component.view().installed.version, component.view().previous.version, component.view().available], ['0.1.9', '0.2.0', true]);
  assert.deepEqual(await component.remove(), { removed: '0.1.9' });
  assert.deepEqual([component.view().installed, component.view().previous], [null, null]);
  await assert.rejects(component.rollback(), { code: 'no-previous' });

  // A look at the releases is kept across a start, and a half-unpacked one is cleared.
  fs.mkdirSync(path.join(root, 'evo.new'));
  const again = new Component({ dir: evo, stateDir: path.join(root, 'state'), glass: '0.8.0', least: '0.1.9', arch: () => 'arm', logger: component.logger });
  await again.init();
  assert.equal(again.view().latest.version, '0.2.0');
  assert.ok(!fs.existsSync(path.join(root, 'evo.new')));
  fs.rmSync(root, { recursive: true, force: true });
});

test('the way back to the stable release puts an older component in place, checked as any', async () => {
  const { root, component, offer, job } = rig();
  await component.init();
  // A test release is here.
  offer('0.1.12', { requires: '0.8.0' });
  await component.check(true);
  await component.install(job());
  // The stable release is older. An update does not step back; the way back does.
  offer('0.1.10', { requires: '0.8.0' });
  const stable = await component.stable();
  assert.equal(stable.version, '0.1.10');
  await component.check(true);
  await assert.rejects(component.install(job()), { code: 'up-to-date' });
  assert.deepEqual(await component.installRelease(job(), stable, { glass: '0.8.0', least: '0.1.9' }), { from: '0.1.12', to: '0.1.10' });
  assert.equal(fs.readFileSync(path.join(root, 'evo/bin/arm/glass-evo'), 'utf8'), 'binary 0.1.10 for arm');
  assert.deepEqual([component.view().installed.version, component.view().previous.version], ['0.1.10', '0.1.12']);
  // And the one before goes back in if the rest of the act fails.
  assert.deepEqual(await component.rollback(), { from: '0.1.10', to: '0.1.12' });
  // A stable release the Glass going in does not work with is not installed.
  await assert.rejects(component.installRelease(job(), stable, { glass: '0.8.0', least: '0.1.11' }), { code: 'too-old' });
  assert.equal(component.view().installed.version, '0.1.12', 'what is here stays');
  await assert.rejects(component.installRelease(job(), null), { code: 'no-release' });
  fs.rmSync(root, { recursive: true, force: true });
});

test('a component that does not check out is not installed, and what is here stays', async () => {
  const { root, component, offer, job } = rig();
  await component.init();
  offer('0.1.9', {});
  await component.check(true);
  await component.install(job());
  const still = function (why) {
    assert.equal(component.view().installed.version, '0.1.9', why);
    assert.ok(!fs.existsSync(path.join(root, 'evo.new')), why + ': nothing half unpacked is left');
    assert.deepEqual(fs.readdirSync(path.join(root, 'state')), ['latest.json'], why + ': no download is left');
  };
  offer('0.2.0', {}, 'e'.repeat(64));
  await component.check(true);
  await assert.rejects(component.install(job()), { code: 'checksum' });
  still('a zip that is not the release\'s');
  offer('0.2.0', { wrongBinary: true });
  await component.check(true);
  await assert.rejects(component.install(job()), { code: 'checksum' });
  still('a binary that is not the manifest\'s');
  offer('0.2.0', { says: '0.2.1' });
  await component.check(true);
  await assert.rejects(component.install(job()), { code: 'bad-manifest' });
  still('a manifest of another version');
  offer('0.2.0', { requires: '0.9.0' });
  await component.check(true);
  await assert.rejects(component.install(job()), { code: 'needs-glass', data: { error: 'needs-glass', needs: '0.9.0' } });
  still('a component for a later Glass');
  // The same one goes in when it is the Glass about to be installed that it must go with.
  assert.deepEqual(await component.install(job(), { glass: '0.9.0', least: '0.2.0' }), { from: '0.1.9', to: '0.2.0' });
  // And not one older than that Glass works with.
  offer('0.2.1', { requires: '0.9.0' });
  await component.check(true);
  await assert.rejects(component.install(job(), { glass: '0.9.0', least: '0.3.0' }), { code: 'too-old' });
  assert.equal(component.view().installed.version, '0.2.0');
  fs.rmSync(root, { recursive: true, force: true });
});

test('what this Glass does not work with is neither offered nor put back', async () => {
  const { root, component, offer, job } = rig({ least: '0.2.0' });
  await component.init();
  offer('0.1.9', {});
  const view = await component.check(true);
  assert.deepEqual([view.available, view.behind], [false, true], 'the latest release is older than this Glass works with');
  await assert.rejects(component.install(job()), { code: 'too-old' });
  assert.equal(component.view().installed, null);
  // One placed by an earlier Glass shows as outdated; the one before it is not a way back.
  fs.mkdirSync(path.join(root, 'evo/bin/arm'), { recursive: true });
  fs.writeFileSync(path.join(root, 'evo/manifest.json'), JSON.stringify({ name: 'glass-evo', version: '0.1.9', binaries: { arm: { path: 'bin/arm/glass-evo', sha256: 'a'.repeat(64) } } }));
  fs.writeFileSync(path.join(root, 'evo/bin/arm/glass-evo'), 'x');
  fs.cpSync(path.join(root, 'evo'), path.join(root, 'evo.prev'), { recursive: true });
  assert.deepEqual([component.view().outdated, component.view().installed.available], [true, true]);
  await assert.rejects(component.rollback(), { code: 'too-old' });
  assert.equal(installedAt(path.join(root, 'evo'), 'x64').available, false, 'installed, with no binary for another architecture');
  fs.rmSync(root, { recursive: true, force: true });
});

test('a Glass about to go in brings the component with it, or does not go in', async () => {
  const { root, component, offer, job } = rig();
  await component.init();
  offer('0.1.9', { requires: '0.8.0' });
  await component.check(true);
  // A Glass's zip as staged for the plugin manager, naming the least glass-evo it works with.
  const staged = function (version, least) {
    const file = path.join(root, 'glass-' + version + '.zip');
    fs.writeFileSync(file, buildZip([{ name: 'index.js', data: '// glass' }, { name: 'package.json', data: JSON.stringify(least ? { name: 'glass', glassEvo: { least: least } } : { name: 'glass' }) }]));
    return { file: file, version: version };
  };
  assert.equal(await pair(component, staged('0.9.0', '0.2.0'), job()), false, 'no component here, nothing to bring');
  await component.install(job());

  assert.equal(await pair(component, staged('0.8.1', '0.1.9'), job()), false, 'the one here already goes with it');
  assert.equal(await pair(component, staged('0.8.1', null), job()), false, 'a Glass that names none');
  await assert.rejects(pair(component, staged('0.7.97', null), job()), { code: 'pair', message: /needs Glass 0\.8\.0 or later/ });

  // The Glass needs 0.2.0; the latest released is still 0.1.9.
  await assert.rejects(pair(component, staged('0.9.0', '0.2.0'), job()), { code: 'pair', message: /needs glass-evo 0\.2\.0 or later.*latest glass-evo released is 0\.1\.9.*nothing was changed/ });
  assert.equal(component.view().installed.version, '0.1.9');

  // 0.2.0 is out and needs that very Glass, later than the one running: it goes in for it.
  offer('0.2.0', { requires: '0.9.0' });
  assert.equal(await pair(component, staged('0.9.0', '0.2.0'), job()), true);
  assert.deepEqual([component.view().installed.version, component.view().previous.version], ['0.2.0', '0.1.9']);
  // The Glass then failed to go in: the component goes back to the one that runs with this Glass.
  component.least = '0.1.9';
  assert.deepEqual(await component.rollback(), { from: '0.2.0', to: '0.1.9' });
  fs.rmSync(root, { recursive: true, force: true });
});

test('the face for a browser is carried out of the component, checked, where the manifest names one', async function () {
  const r = rig();
  await r.component.init();
  // A component that names none has none, and installs as ever.
  r.offer('0.1.13');
  await r.component.check(true);
  await r.component.install(r.job());
  assert.equal(r.component.installed().face, null);
  // One that carries it: the file under face/, as the manifest says it is.
  r.offer('0.1.14', { module: {} });
  await r.component.check(true);
  await r.component.install(r.job());
  const here = r.component.installed();
  assert.equal(here.version, '0.1.14');
  assert.equal(here.face, path.join(r.root, 'evo', 'face', 'glass-evo-face.wasm'));
  assert.equal(fs.readFileSync(here.face, 'utf8'), 'the face for a browser, 0.1.14');
  // Named and not what it is said to be, named and not there, or named
  // outside face/: the component is not installed, and the one before stays.
  for (const [module, code] of [[{ wrong: true }, 'checksum'], [{ absent: true }, 'no-module']]) {
    r.offer('0.1.15', { module: module });
    await r.component.check(true);
    await assert.rejects(r.component.install(r.job()), function (e) { return e.code === code; });
    assert.equal(r.component.installed().version, '0.1.14');
  }
  r.offer('0.1.15', { module: { path: '../face.wasm' } });
  await r.component.check(true);
  await r.component.install(r.job());
  assert.equal(r.component.installed().version, '0.1.15');
  assert.equal(r.component.installed().face, null, 'a module named outside face/ is no module');
});


test('a component zip brought by hand is held to its release, then in place; with a Glass by hand it goes in with it or is asked for', async () => {
  // The releases as GitHub would answer a lookup by tag, one per version made.
  const shelf = {};
  const { root, component, job } = rig({
    releasesUrl: 'r/latest',
    fetch: async function (url) {
      const v = url.replace(/^.*\/v/, '');
      if (!shelf[v]) throw new Error('no such release');
      return { body: Buffer.from(JSON.stringify(release(v, shelf[v].zip, shelf[v].digest))) };
    }
  });
  await component.init();
  const made = function (version, options, digest) {
    const zip = componentZip(version, options);
    shelf[version] = { zip: zip, digest: digest };
    const file = path.join(root, 'up-' + version + '.zip');
    fs.writeFileSync(file, zip);
    return file;
  };
  // Installed from a file, held to the release of its version.
  assert.deepStrictEqual(await component.installFile(job(), made('0.1.10', { requires: '0.8.0' })), { from: null, to: '0.1.10' });
  assert.equal(component.installed().version, '0.1.10');
  assert.ok(!fs.existsSync(path.join(root, 'up-0.1.10.zip')), 'the file is gone after');
  // Not the release as published: refused, nothing changed, the file gone.
  const bad = made('0.1.11', { requires: '0.8.0' }, 'f'.repeat(64));
  await assert.rejects(component.installFile(job(), bad), { code: 'checksum' });
  assert.equal(component.installed().version, '0.1.10');
  assert.ok(!fs.existsSync(bad));
  // A release that cannot be looked up: unverified, never installed.
  const lost = path.join(root, 'lost.zip');
  fs.writeFileSync(lost, componentZip('0.1.12'));
  await assert.rejects(component.installFile(job(), lost), { code: 'unverified' });
  // The version installed: nothing to do.
  await assert.rejects(component.installFile(job(), made('0.1.10', { requires: '0.8.0' })), { code: 'same-version' });
  // A Glass zip in the glass-evo slot is named as such.
  const glassZip = path.join(root, 'glass.zip');
  fs.writeFileSync(glassZip, buildZip([{ name: 'package.json', data: '{"name":"glass"}' }]));
  await assert.rejects(component.installFile(job(), glassZip), { code: 'bad-manifest', message: /Glass release/ });
  // Examined and held for a Glass by hand: its version, the file kept.
  const held = await component.examineFile(job(), made('0.2.0', { requires: '0.9.0' }));
  assert.equal(held.version, '0.2.0');
  assert.ok(fs.existsSync(held.file));
  // A Glass by hand that needs 0.2.0: asked for without a file, taken with one.
  const staged = function (version, least) {
    const file = path.join(root, 'glass-' + version + '.zip');
    fs.writeFileSync(file, buildZip([{ name: 'package.json', data: JSON.stringify({ name: 'glass', glassEvo: { least: least } }) }]));
    return { file: file, version: version };
  };
  await assert.rejects(pair(component, staged('0.9.0', '0.2.0'), job(), { byHand: true }), { code: 'needs-evo', message: /needs glass-evo 0\.2\.0 or later/ });
  assert.equal(component.installed().version, '0.1.10');
  assert.equal(await pair(component, staged('0.9.0', '0.2.0'), job(), { byHand: true, file: held.file }), true);
  assert.deepStrictEqual([component.installed().version, component.previous().version], ['0.2.0', '0.1.10']);
  fs.rmSync(root, { recursive: true, force: true });
});
