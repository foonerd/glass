'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const { buildZip } = require('./zipwriter');
const zip = require('../zip');

const METERS = '[current]\nmeter = a\n\n[gold]\nmeter.type = circular\n\n[black-white]\nmeter.type = linear\n';
const SPECTRUM = '[blue_spectrum]\norigin.x = 0\n';

async function tempDir() {
  return fsp.mkdtemp(path.join(os.tmpdir(), 'glass-zip-'));
}

async function withZip(files, fn) {
  const dir = await tempDir();
  const file = path.join(dir, 'theme.zip');
  await fsp.writeFile(file, buildZip(files));
  const z = await zip.Zip.open(file);
  try {
    await fn(z, dir);
  } finally {
    await z.close();
    await fsp.rm(dir, { recursive: true, force: true });
  }
}

test('one folder layout: a unit per meters.txt with its sections', async function () {
  await withZip([
    { name: '800x480_wood/', data: '' },
    { name: '800x480_wood/meters.txt', data: METERS },
    { name: '800x480_wood/bgr.png', data: Buffer.alloc(100, 7), stored: true }
  ], async function (z) {
    const found = await zip.unitsOf(z, '800x480_wood');
    assert.deepEqual(found.problems, []);
    assert.equal(found.units.length, 1);
    assert.deepEqual(found.units[0], { kind: 'meter', install: 'templates', from: '800x480_wood/', folder: '800x480_wood', names: ['gold', 'black-white'] });
  });
});

test('files at the root take the zip name as their folder', async function () {
  await withZip([
    { name: 'meters.txt', data: METERS },
    { name: 'face.png', data: 'x' }
  ], async function (z) {
    const found = await zip.unitsOf(z, '1920x440_Aureon');
    assert.equal(found.units[0].from, '');
    assert.equal(found.units[0].folder, '1920x440_Aureon');
  });
});

test('a templates/ container names its unit after the zip, a meter and spectrum pair gives two units', async function () {
  await withZip([
    { name: '1920x440_Blue/templates/meters.txt', data: METERS },
    { name: '1920x440_Blue/templates_spectrum/spectrum.txt', data: SPECTRUM }
  ], async function (z) {
    const found = await zip.unitsOf(z, '1920x440_Blue');
    assert.deepEqual(found.problems, []);
    assert.deepEqual(found.units.map(function (u) { return [u.kind, u.install, u.from, u.folder]; }), [
      ['meter', 'templates', '1920x440_Blue/templates/', '1920x440_Blue'],
      ['spectrum', 'templates_spectrum', '1920x440_Blue/templates_spectrum/', '1920x440_Blue']
    ]);
    assert.deepEqual(found.units[1].names, ['blue_spectrum']);
  });
});

test('a bundle gives one unit per inner folder', async function () {
  await withZip([
    { name: 'bundle/one_a/meters.txt', data: METERS },
    { name: 'bundle/one_b/meters.txt', data: METERS }
  ], async function (z) {
    const found = await zip.unitsOf(z, 'bundle');
    assert.deepEqual(found.units.map(function (u) { return u.folder; }), ['one_a', 'one_b']);
  });
});

test('unsafe names and bad folder names are problems, not units', async function () {
  await withZip([
    { name: '../escape/meters.txt', data: METERS },
    { name: 'bad name!/meters.txt', data: METERS },
    { name: 'fine_1/meters.txt', data: METERS }
  ], async function (z) {
    const found = await zip.unitsOf(z, 'x');
    assert.equal(found.units.length, 1);
    assert.equal(found.units[0].folder, 'fine_1');
    assert.equal(found.problems.length, 2);
  });
});

test('a zip without meters.txt or spectrum.txt is not a theme', async function () {
  await withZip([{ name: 'readme.txt', data: 'hello' }], async function (z) {
    const found = await zip.unitsOf(z, 'x');
    assert.deepEqual(found.units, []);
    assert.match(found.problems[0], /no meters.txt/);
  });
});

test('read checks size and checksum, stored and deflated alike', async function () {
  const big = Buffer.alloc(200000);
  for (let i = 0; i < big.length; i++) big[i] = (i * 31) & 0xff;
  await withZip([
    { name: 'a.bin', data: big },
    { name: 'b.bin', data: big, stored: true }
  ], async function (z) {
    assert.equal((await z.read(z.entries[0])).equals(big), true);
    assert.equal((await z.read(z.entries[1])).equals(big), true);
    z.entries[0].crc ^= 1;
    await assert.rejects(z.read(z.entries[0]), /checksum/);
  });
});

test('extractUnit writes the unit under its folder, keeps relative paths, skips escapes and directories', async function () {
  await withZip([
    { name: 't/templates/1280x720_x/', data: '' },
    { name: 't/templates/1280x720_x/meters.txt', data: METERS },
    { name: 't/templates/1280x720_x/fonts/a.ttf', data: 'font' },
    { name: 't/templates/1280x720_x/.DS_Store', data: 'mac' },
    { name: 't/templates/1280x720_x/link', data: 'target', mode: 0o120777 },
    { name: 't/other.txt', data: 'not in the unit' }
  ], async function (z, dir) {
    const found = await zip.unitsOf(z, 't');
    const root = path.join(dir, 'templates');
    await fsp.mkdir(root);
    const result = await zip.extractUnit(z, found.units[0], root);
    assert.equal(result.folder, '1280x720_x');
    assert.equal(result.files, 2);
    assert.equal(fs.readFileSync(path.join(root, '1280x720_x', 'meters.txt'), 'utf8'), METERS);
    assert.equal(fs.readFileSync(path.join(root, '1280x720_x', 'fonts', 'a.ttf'), 'utf8'), 'font');
    assert.equal(fs.existsSync(path.join(root, '1280x720_x', '.DS_Store')), false);
    assert.equal(fs.existsSync(path.join(root, '1280x720_x', 'link')), false);
    assert.equal(fs.existsSync(path.join(root, 'other.txt')), false);
    assert.deepEqual(fs.readdirSync(root), ['1280x720_x']);
  });
});

test('extractUnit replaces an installed folder whole', async function () {
  await withZip([
    { name: '800x480_a/meters.txt', data: METERS },
    { name: '800x480_a/new.png', data: 'new' }
  ], async function (z, dir) {
    const root = path.join(dir, 'templates');
    await fsp.mkdir(path.join(root, '800x480_a'), { recursive: true });
    await fsp.writeFile(path.join(root, '800x480_a', 'old.png'), 'old');
    const found = await zip.unitsOf(z, '800x480_a');
    await zip.extractUnit(z, found.units[0], root);
    assert.deepEqual(fs.readdirSync(path.join(root, '800x480_a')).sort(), ['meters.txt', 'new.png']);
    assert.deepEqual(fs.readdirSync(root), ['800x480_a']);
  });
});

test('a damaged zip is refused', async function () {
  const dir = await tempDir();
  const file = path.join(dir, 'bad.zip');
  await fsp.writeFile(file, Buffer.from('this is not a zip file, not even close to one'));
  await assert.rejects(zip.Zip.open(file), function (e) { return e.code === 'not-a-zip'; });
  await fsp.rm(dir, { recursive: true, force: true });
});

test('safeName and safeFolderName', function () {
  assert.equal(zip.safeName('a/b.png'), true);
  assert.equal(zip.safeName('a/'), true);
  assert.equal(zip.safeName('/a'), false);
  assert.equal(zip.safeName('a/../b'), false);
  assert.equal(zip.safeName('a//b'), false);
  assert.equal(zip.safeName('a\\b'), false);
  assert.equal(zip.safeFolderName('1280x720_g5_450_sm'), true);
  assert.equal(zip.safeFolderName('1280x400_rose rs150'), true);
  assert.equal(zip.safeFolderName('.hidden'), false);
  assert.equal(zip.safeFolderName('a/b'), false);
  assert.equal(zip.safeFolderName(''), false);
});
