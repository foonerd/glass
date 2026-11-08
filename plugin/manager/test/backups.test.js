'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const backups = require('../backups');

// A player's settings as files, with or without a forecast place.
function player(withWeather) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-backups-'));
  const sources = {
    config: path.join(dir, 'configuration', 'config.json'),
    meter: path.join(dir, 'plugin', 'config', 'meter.txt'),
    spectrum: path.join(dir, 'plugin', 'config', 'spectrum.txt'),
    weather: path.join(dir, 'internal', 'weather.json')
  };
  fs.mkdirSync(path.dirname(sources.config), { recursive: true });
  fs.mkdirSync(path.dirname(sources.meter), { recursive: true });
  fs.mkdirSync(path.dirname(sources.weather), { recursive: true });
  fs.writeFileSync(sources.config, JSON.stringify({ testReleases: { type: 'boolean', value: true } }));
  fs.writeFileSync(sources.meter, '[current]\nmeter = random\nface.idle.picture = mine.jpg\n');
  fs.writeFileSync(sources.spectrum, '[current]\nspectrum = s.7\n');
  if (withWeather) fs.writeFileSync(sources.weather, JSON.stringify({ place: { name: 'Lisbon', latitude: 38.7, longitude: -9.1 }, unit: 'C', reading: null }));
  return { dir, sources, backupsDir: path.join(dir, 'internal', 'backups') };
}

test('a backup takes the three files the player always has and the forecast when there is one, and says so in its manifest', () => {
  const p = player(true);
  const made = backups.create(path.join(p.backupsDir, 'mine'), p.sources, { name: 'mine', pluginVersion: '0.11.32', automatic: false });
  assert.deepEqual(made, { ok: true, files: ['config.json', 'peppymeter_config.txt', 'spectrum_config.txt', 'weather.json'] });
  const manifest = JSON.parse(fs.readFileSync(path.join(p.backupsDir, 'mine', 'manifest.json'), 'utf8'));
  assert.equal(manifest.schema_version, backups.SCHEMA_VERSION);
  assert.equal(manifest.name, 'mine');
  assert.equal(manifest.automatic, false);
  assert.deepEqual(manifest.files, made.files);
  assert.match(fs.readFileSync(path.join(p.backupsDir, 'mine', 'weather.json'), 'utf8'), /Lisbon/);
  // Without a place the backup is three files, and the manifest says three.
  const q = player(false);
  const three = backups.create(path.join(q.backupsDir, 'plain'), q.sources, { name: 'plain', automatic: true });
  assert.deepEqual(three.files, ['config.json', 'peppymeter_config.txt', 'spectrum_config.txt']);
  assert.equal(JSON.parse(fs.readFileSync(path.join(q.backupsDir, 'plain', 'manifest.json'), 'utf8')).automatic, true);
  // A required file missing is refused by name, and nothing is written.
  fs.unlinkSync(q.sources.spectrum);
  assert.deepEqual(backups.create(path.join(q.backupsDir, 'short'), q.sources, { name: 'short' }), { error: 'GLASS.BACKUP_SOURCE_MISSING', message: 'spectrum_config.txt' });
  assert.ok(!fs.existsSync(path.join(q.backupsDir, 'short')));
  fs.rmSync(p.dir, { recursive: true, force: true });
  fs.rmSync(q.dir, { recursive: true, force: true });
});

test('a restore puts back what the backup has and leaves alone what it lacks: a backup from before the forecast rode along keeps the place as it stands', () => {
  const p = player(true);
  backups.create(path.join(p.backupsDir, 'withplace'), p.sources, { name: 'withplace' });
  // The place turned off, the theme changed: the restore brings both back.
  fs.writeFileSync(p.sources.weather, JSON.stringify({ place: null, unit: 'F', reading: null }));
  fs.writeFileSync(p.sources.meter, '[current]\nmeter = other\n');
  const back = backups.restore(path.join(p.backupsDir, 'withplace'), p.sources);
  assert.deepEqual(back.restored, ['config', 'meter', 'spectrum', 'weather']);
  assert.deepEqual(back.left, []);
  assert.match(fs.readFileSync(p.sources.weather, 'utf8'), /Lisbon/);
  assert.match(fs.readFileSync(p.sources.meter, 'utf8'), /meter = random/);
  // A schema-1 backup, as every backup before 0.11.32 is: three files, no word on the place.
  const old = path.join(p.backupsDir, 'before-0.11.31');
  fs.mkdirSync(old, { recursive: true });
  fs.writeFileSync(path.join(old, 'config.json'), '{"a":1}');
  fs.writeFileSync(path.join(old, 'peppymeter_config.txt'), '[current]\nmeter = old\n');
  fs.writeFileSync(path.join(old, 'spectrum_config.txt'), '[current]\n');
  fs.writeFileSync(path.join(old, 'manifest.json'), JSON.stringify({ schema_version: 1, plugin_version: '0.11.31', created: '2026-11-08T00:00:00Z', name: 'before-0.11.31', files: ['config.json', 'peppymeter_config.txt', 'spectrum_config.txt'] }));
  const older = backups.restore(old, p.sources);
  assert.deepEqual(older.restored, ['config', 'meter', 'spectrum']);
  assert.deepEqual(older.left, ['weather']);
  assert.match(fs.readFileSync(p.sources.weather, 'utf8'), /Lisbon/, 'the place stands');
  assert.match(fs.readFileSync(p.sources.meter, 'utf8'), /meter = old/);
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('a backup is held to its rules before anything is touched: the manifest, the schema, the files, each file parsable', () => {
  const p = player(true);
  const dir = path.join(p.backupsDir, 'held');
  backups.create(dir, p.sources, { name: 'held' });
  assert.equal(backups.check(dir).ok, true);
  assert.deepEqual(backups.check(path.join(p.backupsDir, 'none')), { error: 'GLASS.BACKUP_NOT_FOUND' });
  const manifest = path.join(dir, 'manifest.json');
  const good = fs.readFileSync(manifest, 'utf8');
  fs.writeFileSync(manifest, '{not json');
  assert.deepEqual(backups.check(dir), { error: 'GLASS.BACKUP_MANIFEST_INVALID' });
  fs.writeFileSync(manifest, JSON.stringify({ schema_version: 99 }));
  assert.deepEqual(backups.check(dir), { error: 'GLASS.BACKUP_SCHEMA_UNSUPPORTED' });
  fs.writeFileSync(manifest, good);
  fs.writeFileSync(path.join(dir, 'weather.json'), '{broken');
  assert.deepEqual(backups.check(dir), { error: 'GLASS.BACKUP_WEATHER_CORRUPT' });
  fs.writeFileSync(path.join(dir, 'weather.json'), '{}');
  fs.writeFileSync(path.join(dir, 'config.json'), '{broken');
  assert.deepEqual(backups.check(dir), { error: 'GLASS.BACKUP_CONFIG_CORRUPT' });
  fs.writeFileSync(path.join(dir, 'config.json'), '{}');
  fs.unlinkSync(path.join(dir, 'spectrum_config.txt'));
  assert.deepEqual(backups.check(dir), { error: 'GLASS.BACKUP_FILES_MISSING' });
  // A restore refused touches nothing.
  const before = fs.readFileSync(p.sources.meter, 'utf8');
  assert.deepEqual(backups.restore(dir, p.sources), { error: 'GLASS.BACKUP_FILES_MISSING' });
  assert.equal(fs.readFileSync(p.sources.meter, 'utf8'), before);
  fs.rmSync(p.dir, { recursive: true, force: true });
});

test('a backup brought from another player is taken in whole, the forecast with it, under a safe and unique name', () => {
  const p = player(true);
  const staging = path.join(p.dir, 'staging');
  backups.create(staging, p.sources, { name: 'From the kitchen' });
  const first = backups.adopt(staging, p.backupsDir, '');
  assert.equal(first.ok, true);
  assert.equal(first.name, 'From the kitchen');
  assert.deepEqual(first.files, ['config.json', 'peppymeter_config.txt', 'spectrum_config.txt', 'weather.json']);
  assert.ok(fs.existsSync(path.join(p.backupsDir, 'From the kitchen', 'weather.json')));
  const manifest = JSON.parse(fs.readFileSync(path.join(p.backupsDir, 'From the kitchen', 'manifest.json'), 'utf8'));
  assert.ok(manifest.imported, 'the manifest says when it was taken in');
  // The same again lands beside it with a number; a wanted name is cleaned; a bad one falls back.
  assert.equal(backups.adopt(staging, p.backupsDir, '').name, 'From the kitchen-2');
  assert.equal(backups.adopt(staging, p.backupsDir, '../evil/name?').name, 'evil-name-');
  assert.equal(backups.adopt(path.join(p.dir, 'nowhere'), p.backupsDir, 'x').error, 'GLASS.BACKUP_MANIFEST_INVALID');
  fs.rmSync(p.dir, { recursive: true, force: true });
});
