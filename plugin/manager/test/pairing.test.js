'use strict';

// The file path's pairing rule replayed row by row from the state table:
// every way a Glass zip or a glass-evo zip can meet the player, what is
// kept from before and what is online, and what is to happen.

const test = require('node:test');
const assert = require('node:assert');
const { plan, fits } = require('../pairing');

const player = (over) => Object.assign({ glass: '0.10.30', evo: '0.10.30', least: '0.10.30', held: null, online: null }, over || {});
const glassZip = (version, least) => ({ kind: 'glass', version, least });
const evoZip = (version, requires) => ({ kind: 'evo', version, requires });
const heldGlass = (version, least) => ({ kind: 'glass', version, least });
const heldEvo = (version, requires) => ({ kind: 'evo', version, requires });
const act = (p) => plan(p).action;

test('a glass-evo fits a Glass when it is at least the least named and the Glass is at least what it requires', () => {
  assert.ok(fits({ version: '0.10.33', requires: '0.10.33' }, { version: '0.10.33', least: '0.10.33' }));
  assert.ok(fits({ version: '0.10.34', requires: '0.10.33' }, { version: '0.10.35', least: '0.10.33' }));
  assert.ok(!fits({ version: '0.10.32', requires: '0.10.32' }, { version: '0.10.33', least: '0.10.33' }), 'older than the least');
  assert.ok(!fits({ version: '0.10.34', requires: '0.10.34' }, { version: '0.10.33', least: '0.10.33' }), 'needs a later Glass');
  assert.ok(fits({ version: '0.10.33', requires: null }, { version: '0.10.33', least: null }), 'nothing named on either side');
  assert.ok(!fits(null, { version: '0.10.33' }));
});

test('a Glass zip: the version running, no newer glass-evo needed, or needed and kept, fetched or held', () => {
  // The version running: nothing, unless a kept glass-evo that fits the running Glass goes in by itself.
  assert.deepStrictEqual(plan(player({ zip: glassZip('0.10.30', '0.10.30') })), { action: 'nothing', say: 'same-version', dropped: null });
  const moot = plan(player({ zip: glassZip('0.10.30', '0.10.30'), held: heldEvo('0.10.30', '0.10.30') }));
  assert.strictEqual(moot.action, 'nothing', 'the kept glass-evo is what is installed already');
  assert.strictEqual(moot.dropped.version, '0.10.30', 'and is discarded as moot');
  assert.strictEqual(act(player({ evo: '0.10.29', least: '0.10.29', zip: glassZip('0.10.30', '0.10.29'), held: heldEvo('0.10.30', '0.10.30') })), 'install-held', 'the kept glass-evo fits the Glass that runs');
  assert.strictEqual(act(player({ zip: glassZip('0.10.30', '0.10.30'), held: heldEvo('0.10.34', '0.10.34') })), 'nothing', 'a kept glass-evo that needs a later Glass than runs stays kept');
  // Needing no newer glass-evo: the upgrade as today.
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', '0.10.30') })), 'install-glass');
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', null) })), 'install-glass', 'a Glass naming no least');
  assert.strictEqual(act(player({ zip: glassZip('0.10.29', '0.10.29') })), 'install-glass', 'an older Glass, a way back by file');
  // Needing no newer glass-evo, a kept zip that fits: both.
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', '0.10.30'), held: heldEvo('0.10.33', '0.10.33') })), 'install-both');
  // Needing no newer glass-evo, a kept zip that does not fit: Glass alone, the kept zip discarded.
  const dropped = plan(player({ zip: glassZip('0.10.33', '0.10.30'), held: heldEvo('0.10.34', '0.10.34') }));
  assert.strictEqual(dropped.action, 'install-glass');
  assert.strictEqual(dropped.dropped.version, '0.10.34');
  // Needing a newer glass-evo: kept and fitting, both; online, fetched; else held.
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', '0.10.33'), held: heldEvo('0.10.33', '0.10.33') })), 'install-both');
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', '0.10.33'), online: { glass: '0.10.33', evo: '0.10.33' } })), 'install-glass-fetching-evo');
  assert.strictEqual(act(player({ zip: glassZip('0.10.33', '0.10.33'), online: { glass: '0.10.33', evo: '0.10.32' } })), 'hold-glass', 'the latest online is too old');
  assert.deepStrictEqual(plan(player({ zip: glassZip('0.10.33', '0.10.33') })), { action: 'hold-glass', say: 'glass-held', needs: '0.10.33', dropped: null });
  const stale = plan(player({ zip: glassZip('0.10.33', '0.10.33'), held: heldEvo('0.10.31', '0.10.31') }));
  assert.strictEqual(stale.action, 'hold-glass');
  assert.strictEqual(stale.dropped.version, '0.10.31', 'a kept glass-evo too old is discarded');
  // A kept zip of the same kind is not the rule's business.
  assert.deepStrictEqual(plan(player({ zip: glassZip('0.10.33', '0.10.33'), held: heldGlass('0.10.32', '0.10.32') })), { action: 'hold-glass', say: 'glass-held', needs: '0.10.33', dropped: null });
  // A player without glass-evo needs none: the Glass goes in alone, as the online upgrade has it.
  assert.strictEqual(act(player({ evo: null, zip: glassZip('0.10.33', '0.10.33') })), 'install-glass');
  assert.strictEqual(act(player({ evo: null, zip: glassZip('0.10.33', '0.10.33'), held: heldEvo('0.10.33', '0.10.33') })), 'install-both', 'but a kept glass-evo that fits goes in with it');
});

test('a glass-evo zip: installed already, too old for the running Glass, fitting, or needing a later Glass', () => {
  // Installed already: nothing, unless a kept Glass that fits goes in with it.
  assert.deepStrictEqual(plan(player({ zip: evoZip('0.10.30', '0.10.30') })), { action: 'nothing', say: 'same-version', dropped: null });
  assert.strictEqual(act(player({ zip: evoZip('0.10.30', '0.10.30'), held: heldGlass('0.10.33', '0.10.30') })), 'install-both');
  // Older than the running Glass works with: refused, unless a kept Glass fits it.
  assert.deepStrictEqual(plan(player({ zip: evoZip('0.10.29', '0.10.29') })), { action: 'refuse-too-old', say: 'evo-too-old', least: '0.10.30', dropped: null });
  assert.strictEqual(act(player({ zip: evoZip('0.10.29', '0.10.29'), held: heldGlass('0.10.29', '0.10.29') })), 'install-both', 'a kept older Glass that fits: both, a way back by file');
  assert.strictEqual(act(player({ zip: evoZip('0.10.29', '0.10.29'), held: heldGlass('0.10.33', '0.10.33') })), 'refuse-too-old', 'a kept Glass it does not fit changes nothing, and stays kept');
  // Fitting the running Glass: in, as today; a kept Glass that fits: both.
  assert.strictEqual(act(player({ zip: evoZip('0.10.31', '0.10.30') })), 'install-evo');
  assert.strictEqual(act(player({ zip: evoZip('0.10.31', null) })), 'install-evo', 'requiring nothing');
  assert.strictEqual(act(player({ zip: evoZip('0.10.33', '0.10.30'), held: heldGlass('0.10.33', '0.10.33') })), 'install-both');
  const alone = plan(player({ zip: evoZip('0.10.31', '0.10.30'), held: heldGlass('0.10.34', '0.10.34') }));
  assert.strictEqual(alone.action, 'install-evo', 'a kept Glass it does not fit: in alone');
  assert.strictEqual(alone.dropped.version, '0.10.34', 'and the kept Glass, which this glass-evo cannot serve, is discarded');
  // Needing a later Glass: a kept Glass that fits, both; else held, the online word with it.
  assert.strictEqual(act(player({ zip: evoZip('0.10.33', '0.10.33'), held: heldGlass('0.10.33', '0.10.33') })), 'install-both');
  assert.deepStrictEqual(plan(player({ zip: evoZip('0.10.33', '0.10.33') })), { action: 'hold-evo', say: 'evo-held', needs: '0.10.33', online: false, dropped: null });
  assert.strictEqual(plan(player({ zip: evoZip('0.10.33', '0.10.33'), online: { glass: '0.10.33', evo: '0.10.33' } })).online, true, 'Glass can be upgraded on the tab');
  const stale = plan(player({ zip: evoZip('0.10.33', '0.10.33'), held: heldGlass('0.10.32', '0.10.32') }));
  assert.strictEqual(stale.action, 'hold-evo');
  assert.strictEqual(stale.dropped.version, '0.10.32', 'a kept Glass too old is discarded');
  assert.strictEqual(plan(player({ zip: evoZip('0.10.33', '0.10.33'), held: heldEvo('0.10.32', '0.10.32') })).dropped, null, 'a kept zip of the same kind is not the rule\'s business');
  // No glass-evo installed: a zip that fits goes in, one needing a later Glass is held.
  assert.strictEqual(act(player({ evo: null, zip: evoZip('0.10.30', '0.10.30') })), 'install-evo');
  assert.strictEqual(act(player({ evo: null, zip: evoZip('0.10.33', '0.10.33') })), 'hold-evo');
  // The gridlock itself, from 0.9.x: either zip first, the other completes it.
  const nine = { glass: '0.9.39', evo: '0.2.39', least: '0.2.39', held: null, online: null };
  assert.strictEqual(act(Object.assign({}, nine, { zip: glassZip('0.10.33', '0.10.33') })), 'hold-glass');
  assert.strictEqual(act(Object.assign({}, nine, { zip: evoZip('0.10.33', '0.10.33'), held: heldGlass('0.10.33', '0.10.33') })), 'install-both');
  assert.strictEqual(act(Object.assign({}, nine, { zip: evoZip('0.10.33', '0.10.33') })), 'hold-evo');
  assert.strictEqual(act(Object.assign({}, nine, { zip: glassZip('0.10.33', '0.10.33'), held: heldEvo('0.10.33', '0.10.33') })), 'install-both');
});

test('every row of the table has an action, and a plan never names a dropped zip of the kind that came', () => {
  const versions = ['0.10.29', '0.10.30', '0.10.33'];
  const zips = [];
  for (const kind of ['glass', 'evo']) for (const v of versions) for (const need of [null, '0.10.30', '0.10.33']) {
    zips.push(kind === 'glass' ? glassZip(v, need) : evoZip(v, need));
  }
  const helds = [null, heldGlass('0.10.33', '0.10.33'), heldGlass('0.10.29', '0.10.29'), heldEvo('0.10.33', '0.10.33'), heldEvo('0.10.29', '0.10.29')];
  const onlines = [null, { glass: '0.10.33', evo: '0.10.33' }];
  const evos = [null, '0.10.30'];
  const actions = new Set(['nothing', 'install-glass', 'install-evo', 'install-both', 'install-held', 'install-glass-fetching-evo', 'hold-glass', 'hold-evo', 'refuse-too-old']);
  let rows = 0;
  for (const zip of zips) for (const held of helds) for (const online of onlines) for (const evo of evos) {
    const p = plan(player({ zip, held, online, evo }));
    assert.ok(actions.has(p.action), p.action);
    assert.ok(typeof p.say === 'string' && p.say, 'every plan says something');
    assert.ok(!p.dropped || p.dropped.kind !== zip.kind);
    if (p.action === 'install-both') assert.ok(held && held.kind !== zip.kind, 'both takes the kept zip of the other kind');
    rows++;
  }
  assert.ok(rows > 300, 'the table is replayed whole: ' + rows + ' rows');
});
