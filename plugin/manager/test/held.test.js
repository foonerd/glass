'use strict';

// The held zips: kept with their facts, one of each kind, read back after a
// restart, taken out for an install, and discarded.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { Held } = require('../held');

function fresh() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'glass-held-'));
}

function zipAt(dir, name) {
  const file = path.join(dir, name);
  fs.writeFileSync(file, 'PK' + name);
  return file;
}

test('a zip kept is named by what it is, read back after a restart, and replaced by the next of its kind', async () => {
  const dir = fresh();
  const inbox = fresh();
  const held = new Held({ dir: dir });
  await held.init();
  assert.deepEqual(held.summary(), { glass: null, evo: null });
  const entry = await held.keep('glass', zipAt(inbox, 'upload-1.zip'), { version: '0.10.33', least: '0.10.33', verified: 'archive' });
  assert.equal(entry.file, path.join(dir, 'glass-0.10.33.zip'));
  assert.ok(fs.existsSync(entry.file));
  assert.ok(!fs.existsSync(path.join(inbox, 'upload-1.zip')), 'moved, not copied');
  assert.equal(held.get('glass').least, '0.10.33');
  assert.equal(held.summary().glass.needs, '0.10.33');
  // Read back by another instance, as after a restart of the backend.
  const again = new Held({ dir: dir });
  await again.init();
  assert.equal(again.get('glass').version, '0.10.33');
  assert.equal(again.get('evo'), null);
  // The next Glass zip replaces it.
  await again.keep('glass', zipAt(inbox, 'upload-2.zip'), { version: '0.10.34', least: '0.10.34', verified: 'release' });
  assert.equal(again.get('glass').version, '0.10.34');
  assert.ok(!fs.existsSync(path.join(dir, 'glass-0.10.33.zip')), 'the one before is gone');
  assert.ok(fs.existsSync(path.join(dir, 'glass-0.10.34.zip')));
  // A glass-evo beside it, with what it requires.
  await again.keep('evo', zipAt(inbox, 'upload-3.zip'), { version: '0.10.34', requires: '0.10.34', verified: 'archive' });
  assert.deepEqual(Object.keys(again.summary()).sort(), ['evo', 'glass']);
  assert.equal(again.summary().evo.needs, '0.10.34');
  assert.equal(again.get('evo').requires, '0.10.34');
});

test('an entry whose file went missing is forgotten at init, and a bad note is an empty one', async () => {
  const dir = fresh();
  const held = new Held({ dir: dir });
  await held.init();
  await held.keep('evo', zipAt(fresh(), 'u.zip'), { version: '0.10.33', requires: '0.10.33' });
  fs.rmSync(path.join(dir, 'glass-evo-0.10.33.zip'));
  const again = new Held({ dir: dir });
  await again.init();
  assert.equal(again.get('evo'), null);
  fs.writeFileSync(path.join(dir, 'held.json'), '{not json');
  const third = new Held({ dir: dir });
  await third.init();
  assert.deepEqual(third.summary(), { glass: null, evo: null });
});

test('take hands the file over and forgets it; drop removes it; clear empties both', async () => {
  const dir = fresh();
  const held = new Held({ dir: dir });
  await held.init();
  await held.keep('glass', zipAt(fresh(), 'g.zip'), { version: '0.10.33', least: '0.10.33' });
  await held.keep('evo', zipAt(fresh(), 'e.zip'), { version: '0.10.33', requires: '0.10.33' });
  const taken = await held.take('glass');
  assert.equal(taken.version, '0.10.33');
  assert.ok(fs.existsSync(taken.file), 'the file is the caller\'s now');
  assert.equal(held.get('glass'), null);
  assert.equal(await held.take('glass'), null);
  fs.rmSync(taken.file);
  const dropped = await held.drop('evo');
  assert.equal(dropped.version, '0.10.33');
  assert.ok(!fs.existsSync(dropped.file));
  assert.equal(await held.drop('evo'), null);
  await held.keep('evo', zipAt(fresh(), 'e2.zip'), { version: '0.10.34', requires: '0.10.34' });
  await held.clear();
  assert.deepEqual(held.summary(), { glass: null, evo: null });
  assert.deepEqual(fs.readdirSync(dir), ['held.json']);
  await assert.rejects(held.keep('face', zipAt(fresh(), 'x.zip'), { version: '1.0.0' }), /not a kind/);
});
