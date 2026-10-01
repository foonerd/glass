'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { buildZip } = require('./zipwriter');
const { Component, leastOf, manifestOf, misfit, installedAt, pairPlan } = require('../component');

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
  assert.deepEqual(installedAt(path.join(root, 'evo'), 'arm'), { installed: false, available: false, version: null, binary: null, arch: 'arm', requires: null });
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
